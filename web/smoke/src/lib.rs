use futures::{SinkExt, StreamExt};
use irc::connection::{Config, Connection, Security, WebSocket};
use irc::proto;
use wasm_bindgen::prelude::*;

async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

/// Connect, register without any credentials (the server-side gate logs us
/// in), run a WHOIS on ourselves, and return every line received.
#[wasm_bindgen]
pub async fn smoke(server: String, port: u16, secure: bool, path: String) -> Result<js_sys::Array, JsValue> {
    let security = if secure {
        Security::Secured { accept_invalid_certs: false, root_cert_path: None, client_cert_path: None, client_key_path: None }
    } else {
        Security::Unsecured
    };
    let config = Config { server: &server, port, security, proxy: None, websocket: Some(WebSocket { path: &path }) };
    let mut conn = Connection::new(config, irc::Codec::new(None)).await.map_err(|e| JsValue::from_str(&e.to_string()))?;

    for (cmd, params) in [
        ("CAP", vec!["LS", "302"]),
        ("NICK", vec!["webtester"]),
        ("USER", vec!["web", "0", "*", "Halloy web"]),
        ("CAP", vec!["END"]),
    ] {
        conn.send(proto::command(cmd, params.into_iter().map(String::from).collect()))
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
    }

    let out = js_sys::Array::new();
    let mut whois_sent = false;
    let deadline = js_sys::Date::now() + 4000.0;
    while js_sys::Date::now() < deadline {
        let next = futures::future::select(Box::pin(conn.next()), Box::pin(sleep(200))).await;
        match next {
            futures::future::Either::Left((Some(Ok(Ok(msg))), _)) => {
                let line = format!("{} {:?}", msg.command.command(), msg.command.clone().parameters());
                out.push(&JsValue::from_str(&line));
                if !whois_sent && msg.command.command() == "001" {
                    whois_sent = true;
                    let me = msg.command.parameters().first().cloned().unwrap_or_default();
                    conn.send(proto::command("WHOIS", vec![me])).await.map_err(|e| JsValue::from_str(&e.to_string()))?;
                }
            }
            futures::future::Either::Left((Some(Ok(Err(e))), _)) => {
                out.push(&JsValue::from_str(&format!("PARSE-ERROR {e}")));
            }
            futures::future::Either::Left((Some(Err(e)), _)) => {
                out.push(&JsValue::from_str(&format!("STREAM-ERROR {e}")));
                break;
            }
            futures::future::Either::Left((None, _)) => {
                out.push(&JsValue::from_str("CLOSED"));
                break;
            }
            futures::future::Either::Right(_) => {}
        }
    }
    let _ = conn.shutdown().await;
    Ok(out)
}
