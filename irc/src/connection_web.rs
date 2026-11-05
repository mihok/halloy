//! Browser transport for `wasm32` builds (slashcheer fork).
//!
//! Browsers cannot open raw TCP or TLS sockets, so on the web every
//! connection is IRC-over-WebSocket through the browser's own `WebSocket`
//! (TLS is the browser's `wss://`). This module mirrors the public API of the
//! native `connection.rs` so the `data` crate compiles unchanged; anything
//! the browser can't do (proxies, DCC listeners, custom certificates) returns
//! `Error::Unsupported`.
//!
//! Authentication is ambient: the page is same-origin with the server, so the
//! browser attaches the Cloudflare Access cookie to the upgrade request and
//! the server-side gate logs the connection in.

use std::collections::VecDeque;
use std::io;
use std::net::IpAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::BytesMut;
use futures::channel::{mpsc, oneshot};
use futures::{Sink, Stream, StreamExt};
use send_wrapper::SendWrapper;
use tokio_util::codec::{Decoder, Encoder};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{
    BinaryType, CloseEvent, Event, MessageEvent, WebSocket as JsWebSocket,
};

const SUBPROTOCOLS: [&str; 2] = ["text.ircv3.net", "binary.ircv3.net"];

/// No-op on the web (rustls isn't used).
pub fn prepare() {}

#[derive(Debug, Clone)]
pub enum Proxy {
    Http {
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    },
    Socks5 {
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    },
    #[cfg(feature = "tor")]
    Tor,
}

#[derive(Debug, Clone)]
pub enum Security<'a> {
    Unsecured,
    Secured {
        accept_invalid_certs: bool,
        root_cert_path: Option<&'a PathBuf>,
        client_cert_path: Option<&'a PathBuf>,
        client_key_path: Option<&'a PathBuf>,
    },
}

#[derive(Debug, Clone)]
pub struct Config<'a> {
    pub server: &'a str,
    pub port: u16,
    pub security: Security<'a>,
    pub proxy: Option<Proxy>,
    pub websocket: Option<WebSocket<'a>>,
}

#[derive(Debug, Clone)]
pub struct WebSocket<'a> {
    pub path: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Kept for API parity with the native build; never produced here
    /// (the browser handles TLS).
    #[error("tls error: {0}")]
    Tls(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("websocket error: {0}")]
    WebSocket(String),
    #[error("not supported in the browser: {0}")]
    Unsupported(&'static str),
}

enum Incoming {
    Data(Vec<u8>),
    Closed(Option<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Binary,
}

struct Inner {
    socket: JsWebSocket,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        self.socket.set_onerror(None);
        let _ = self.socket.close();
    }
}

/// A browser WebSocket carrying IRC lines, framed with `Codec`.
pub struct Connection<Codec> {
    // The browser's JS objects are !Send; wasm32 is single-threaded, so
    // SendWrapper only asserts we never move them across threads.
    inner: SendWrapper<Inner>,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    codec: Codec,
    read: BytesMut,
    mode: Mode,
    closed: bool,
    pending: VecDeque<Vec<u8>>,
}

fn js_err(v: wasm_bindgen::JsValue) -> Error {
    Error::WebSocket(v.as_string().unwrap_or_else(|| format!("{v:?}")))
}

fn websocket_url(config: &Config<'_>, path: &str) -> String {
    let scheme = match config.security {
        Security::Secured { .. } => "wss",
        Security::Unsecured => "ws",
    };
    let host = if config.server.contains(':') && !config.server.starts_with('[')
    {
        format!("[{}]", config.server)
    } else {
        config.server.to_string()
    };
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let default_port =
        matches!((scheme, config.port), ("wss", 443) | ("ws", 80));
    if default_port {
        format!("{scheme}://{host}{path}")
    } else {
        format!("{scheme}://{host}:{}{path}", config.port)
    }
}

/// Everything that must stay alive until the socket opens. JS objects are
/// !Send, so they're only touched synchronously or inside a SendWrapper,
/// which keeps the connect future Send as the data crate requires.
struct Pending {
    inner: SendWrapper<Inner>,
    on_open: SendWrapper<Closure<dyn FnMut()>>,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    opened: oneshot::Receiver<Result<(), Error>>,
}

fn open_socket(url: &str) -> Result<Pending, Error> {
    let protocols = js_sys::Array::new();
    for p in SUBPROTOCOLS {
        protocols.push(&wasm_bindgen::JsValue::from_str(p));
    }
    let socket =
        JsWebSocket::new_with_str_sequence(url, &protocols).map_err(js_err)?;
    socket.set_binary_type(BinaryType::Arraybuffer);

    let (tx, rx) = mpsc::unbounded::<Incoming>();
    let (open_tx, open_rx) = oneshot::channel::<Result<(), Error>>();
    let open_tx = std::rc::Rc::new(std::cell::RefCell::new(Some(open_tx)));

    let on_message = {
        let tx = tx.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let data = e.data();
            let bytes = if let Some(s) = data.as_string() {
                s.into_bytes()
            } else if let Ok(buf) = data.dyn_into::<js_sys::ArrayBuffer>() {
                js_sys::Uint8Array::new(&buf).to_vec()
            } else {
                return;
            };
            let _ = tx.unbounded_send(Incoming::Data(bytes));
        })
    };
    let on_close = {
        let open_tx = open_tx.clone();
        Closure::<dyn FnMut(CloseEvent)>::new(move |e: CloseEvent| {
            let reason = e.reason();
            let reason = (!reason.is_empty()).then_some(reason);
            if let Some(o) = open_tx.borrow_mut().take() {
                let _ = o.send(Err(Error::WebSocket(
                    reason.clone().unwrap_or_else(|| {
                        format!("closed with code {}", e.code())
                    }),
                )));
            }
            let _ = tx.unbounded_send(Incoming::Closed(reason));
        })
    };
    // The close event that always follows an error carries the details.
    let on_error = Closure::<dyn FnMut(Event)>::new(move |_e: Event| {});
    let on_open = Closure::<dyn FnMut()>::new(move || {
        if let Some(o) = open_tx.borrow_mut().take() {
            let _ = o.send(Ok(()));
        }
    });

    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

    Ok(Pending {
        inner: SendWrapper::new(Inner {
            socket,
            _on_message: on_message,
            _on_close: on_close,
            _on_error: on_error,
        }),
        on_open: SendWrapper::new(on_open),
        incoming: rx,
        opened: open_rx,
    })
}

impl<Codec> Connection<Codec> {
    pub async fn new(config: Config<'_>, codec: Codec) -> Result<Self, Error> {
        if config.proxy.is_some() {
            return Err(Error::Unsupported("proxies"));
        }
        if let Security::Secured {
            client_cert_path: Some(_),
            ..
        }
        | Security::Secured {
            root_cert_path: Some(_),
            ..
        } = config.security
        {
            return Err(Error::Unsupported("custom TLS certificates"));
        }
        // Every web connection is a websocket, whatever the config says.
        let path = config.websocket.as_ref().map_or("/webirc", |w| w.path);
        let url = websocket_url(&config, path);

        let Pending {
            inner,
            on_open,
            incoming,
            opened,
        } = open_socket(&url)?;

        let opened = opened.await.unwrap_or_else(|_| {
            Err(Error::WebSocket("connection dropped".into()))
        });
        inner.socket.set_onopen(None);
        drop(on_open);
        opened?;

        let mode = if inner.socket.protocol() == "text.ircv3.net" {
            Mode::Text
        } else {
            Mode::Binary
        };

        Ok(Self {
            inner,
            incoming,
            codec,
            read: BytesMut::new(),
            mode,
            closed: false,
            pending: VecDeque::new(),
        })
    }

    pub async fn listen_and_accept(
        _address: IpAddr,
        _port: u16,
        _security: Security<'_>,
        _codec: Codec,
    ) -> Result<Self, Error> {
        Err(Error::Unsupported("listening for DCC connections"))
    }

    pub async fn shutdown(&mut self) -> Result<(), Error> {
        self.closed = true;
        self.inner.socket.close().map_err(js_err)
    }

    /// The browser doesn't expose local addresses.
    pub fn local_ip(&self) -> Option<IpAddr> {
        None
    }

    /// Browsers answer server pings themselves and can't send ping frames;
    /// IRC-level PING/PONG still runs.
    pub async fn send_ws_ping(
        &mut self,
        _timeout: Duration,
    ) -> Result<(), io::Error> {
        Ok(())
    }

    fn push_crlf_line(&mut self, bytes: &[u8]) {
        self.read.extend_from_slice(bytes);
        if !self.read.ends_with(b"\r\n") {
            self.read.extend_from_slice(b"\r\n");
        }
    }

    fn flush_pending(&mut self) -> Result<(), io::Error> {
        while let Some(bytes) = self.pending.pop_front() {
            let res = match self.mode {
                Mode::Text => {
                    let text = String::from_utf8(bytes).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, e)
                    })?;
                    self.inner.socket.send_with_str(&text)
                }
                Mode::Binary => self.inner.socket.send_with_u8_array(&bytes),
            };
            res.map_err(|e| {
                io::Error::new(io::ErrorKind::BrokenPipe, format!("{e:?}"))
            })?;
        }
        Ok(())
    }
}

impl<Codec> Stream for Connection<Codec>
where
    Codec: Decoder + Unpin,
    Codec::Error: From<io::Error>,
{
    type Item = Result<Codec::Item, Codec::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(item) = this.codec.decode(&mut this.read)? {
                return Poll::Ready(Some(Ok(item)));
            }
            if this.closed {
                return Poll::Ready(None);
            }
            match this.incoming.poll_next_unpin(cx) {
                Poll::Ready(Some(Incoming::Data(bytes))) => {
                    this.push_crlf_line(&bytes);
                }
                Poll::Ready(Some(Incoming::Closed(reason))) => {
                    this.closed = true;
                    if let Some(reason) = reason {
                        return Poll::Ready(Some(Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            reason,
                        )
                        .into())));
                    }
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<Item, Codec> Sink<Item> for Connection<Codec>
where
    Codec: Encoder<Item> + Unpin,
    Codec::Error: From<io::Error>,
{
    type Error = Codec::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if self.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "websocket closed",
            )
            .into()));
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = self.get_mut();
        let mut bytes = BytesMut::new();
        this.codec.encode(item, &mut bytes)?;
        // websocket frames are message boundaries, so strip CRLF
        if bytes.ends_with(b"\r\n") {
            bytes.truncate(bytes.len() - 2);
        }
        this.pending.push_back(bytes.to_vec());
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        // The browser buffers sends internally; hand everything over now.
        Poll::Ready(self.get_mut().flush_pending().map_err(Into::into))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        let flushed = this.flush_pending();
        this.closed = true;
        let _ = this.inner.socket.close();
        Poll::Ready(flushed.map_err(Into::into))
    }
}
