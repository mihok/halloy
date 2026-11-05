//! Browser-only smoke tests for the web port; empty on desktop builds.
#![cfg(target_arch = "wasm32")]

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
pub async fn smoke(
    server: String,
    port: u16,
    secure: bool,
    path: String,
) -> Result<js_sys::Array, JsValue> {
    let security = if secure {
        Security::Secured {
            accept_invalid_certs: false,
            root_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        }
    } else {
        Security::Unsecured
    };
    let config = Config {
        server: &server,
        port,
        security,
        proxy: None,
        websocket: Some(WebSocket { path: &path }),
    };
    let mut conn = Connection::new(config, irc::Codec::new(None))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    for (cmd, params) in [
        ("CAP", vec!["LS", "302"]),
        ("NICK", vec!["webtester"]),
        ("USER", vec!["web", "0", "*", "Halloy web"]),
        ("CAP", vec!["END"]),
    ] {
        conn.send(proto::command(
            cmd,
            params.into_iter().map(String::from).collect(),
        ))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    }

    let out = js_sys::Array::new();
    let mut whois_sent = false;
    let deadline = js_sys::Date::now() + 4000.0;
    while js_sys::Date::now() < deadline {
        let next = futures::future::select(
            Box::pin(conn.next()),
            Box::pin(sleep(200)),
        )
        .await;
        match next {
            futures::future::Either::Left((Some(Ok(Ok(msg))), _)) => {
                let line = format!(
                    "{} {:?}",
                    msg.command.command(),
                    msg.command.clone().parameters()
                );
                out.push(&JsValue::from_str(&line));
                if !whois_sent && msg.command.command() == "001" {
                    whois_sent = true;
                    let me = msg
                        .command
                        .parameters()
                        .first()
                        .cloned()
                        .unwrap_or_default();
                    conn.send(proto::command("WHOIS", vec![me]))
                        .await
                        .map_err(|e| JsValue::from_str(&e.to_string()))?;
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

/// Exercise data::platform's browser shims at runtime and report each check.
#[wasm_bindgen]
pub async fn platform_check() -> js_sys::Array {
    use std::time::Duration;

    use data::platform::{self, fs, time};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let out = js_sys::Array::new();
    let check = |name: &str, ok: bool, detail: String| {
        out.push(&JsValue::from_str(&format!(
            "{} {name} {detail}",
            if ok { "ok  " } else { "FAIL" }
        )));
    };

    // filesystem
    let r = fs::write("/cfg/missing-parent/x", b"x").await;
    check("write without parent dir errors", r.is_err(), String::new());
    fs::create_dir_all("/cfg/themes").await.unwrap();
    fs::write("/cfg/config.toml", "nick = \"alice\"")
        .await
        .unwrap();
    let s = fs::read_to_string("/cfg/config.toml")
        .await
        .unwrap_or_default();
    check("write + read_to_string", s == "nick = \"alice\"", s.clone());

    let mut f = fs::File::create("/cfg/log.txt").await.unwrap();
    f.write_all(b"line1\n").await.unwrap();
    drop(f);
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open("/cfg/log.txt")
        .await
        .unwrap();
    f.write_all(b"line2\n").await.unwrap();
    let mut buf = String::new();
    fs::File::open("/cfg/log.txt")
        .await
        .unwrap()
        .read_to_string(&mut buf)
        .await
        .unwrap();
    check(
        "File create + append + read",
        buf == "line1\nline2\n",
        format!("{buf:?}"),
    );

    let len = fs::metadata("/cfg/log.txt")
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    check("metadata len", len == 12, len.to_string());

    fs::write("/cfg/themes/dark.toml", "a").await.unwrap();
    fs::write("/cfg/themes/light.toml", "b").await.unwrap();
    let mut names: Vec<String> =
        fs::ReadDirStream::new(fs::read_dir("/cfg/themes").await.unwrap())
            .filter_map(|e| async move {
                e.ok().map(|e| e.file_name().to_string_lossy().into_owned())
            })
            .collect()
            .await;
    names.sort();
    check(
        "read_dir",
        names == ["dark.toml", "light.toml"],
        format!("{names:?}"),
    );

    fs::rename("/cfg/themes/light.toml", "/cfg/themes/day.toml")
        .await
        .unwrap();
    fs::remove_file("/cfg/themes/dark.toml").await.unwrap();
    let gone = !fs::try_exists("/cfg/themes/dark.toml").await.unwrap();
    let moved = fs::read_to_string("/cfg/themes/day.toml")
        .await
        .unwrap_or_default()
        == "b";
    check("rename + remove_file", gone && moved, String::new());

    // timers (tokio's would panic here: no runtime, no timer driver)
    let start = web_time::Instant::now();
    time::sleep(Duration::from_millis(60)).await;
    let ms = start.elapsed().as_millis();
    check("sleep 60ms", (55..400).contains(&ms), format!("{ms}ms"));

    let r = time::timeout(
        Duration::from_millis(40),
        futures::future::pending::<()>(),
    )
    .await;
    check("timeout on a pending future", r.is_err(), String::new());

    let start = web_time::Instant::now();
    let mut iv = time::interval(Duration::from_millis(30));
    for _ in 0..3 {
        iv.tick().await;
    }
    let ms = start.elapsed().as_millis();
    check(
        "interval: 3 ticks (first immediate)",
        (55..400).contains(&ms),
        format!("{ms}ms"),
    );

    // tasks
    let flag = std::rc::Rc::new(std::cell::Cell::new(0));
    let f2 = flag.clone();
    platform::spawn(async move { f2.set(1) });
    let f3 = flag.clone();
    let handle = platform::spawn_abortable(async move {
        time::sleep(Duration::from_millis(30)).await;
        f3.set(99);
    });
    handle.abort();
    time::sleep(Duration::from_millis(60)).await;
    check(
        "spawn runs, abort cancels",
        flag.get() == 1,
        flag.get().to_string(),
    );

    let v = platform::spawn_blocking(|| 21 * 2).await;
    check("spawn_blocking inline", matches!(v, Ok(42)), String::new());

    out
}
