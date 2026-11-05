# Halloy in the browser — porting plan

Goal: our own build of [Halloy](https://github.com/squidowl/halloy) (iced, Rust)
compiled to WebAssembly and served at `https://<irc_hostname>/client/`, signed
in automatically through the site's GitHub/Google login.

This fork (`mihok/halloy`) carries the web port. Branches:

- `main` mirrors upstream [squidowl/halloy](https://github.com/squidowl/halloy). Sync it with
  `git fetch upstream && git merge upstream/main` (add the remote once:
  `git remote add upstream https://github.com/squidowl/halloy`).
- `web-port` holds our changes, merged forward from `main` after each sync.

Keep changes small and `cfg(target_arch = "wasm32")`-gated so upstream merges stay easy.

The server side (Ergo on Cloudflare, the login gate, the site that will host
the build at `/client/`) lives in [`mihok/slashcheer-irc`](https://github.com/mihok/slashcheer-irc).

License: Halloy is GPL-3.0-or-later. A modified build we serve to users must
come with its source; this repository is that source.

## Why login needs no client changes

The slashcheer-irc Worker's `IrcGate` (`worker/src/gate.ts` there) logs every websocket in as the
account bound to the visitor's Cloudflare Access identity. The browser sends
the Access cookie automatically on the same-origin `wss://<host>/webirc`
upgrade, so the web build needs **no credentials, SASL, or login UI**: it
connects with any nickname and the server makes it the user's account. The
only client-side work is a transport.

## Status

| Piece | State |
|---|---|
| `irc` crate on `wasm32` | **Done.** `irc/src/connection_web.rs` is a browser `WebSocket` transport with the same API as the native `connection.rs` (selected in `irc/src/lib.rs` with `#[cfg(target_arch = "wasm32")]`). Proxies/DCC/custom certs return `Error::Unsupported`. Native build and tests unchanged. |
| End-to-end check | **Done.** `web/smoke/`: Halloy's `irc::Connection` + `irc::Codec` compiled to wasm, run in headless Chromium, through the real `IrcGate` to Ergo 2.19.1. Logged in as the bound account with no credentials, nick forced, `+Z` secure, real IP forwarded. |
| iced / wgpu / winit on wasm | Compile (seen during `cargo check -p data --target wasm32-unknown-unknown`). Rendering not yet tried. |
| `data` crate on wasm | **Next.** First error: tokio features `fs`/`process` (tokio on wasm supports only sync, macros, io-util, rt, time). |
| `halloy` app crate on wasm | Not started. |
| Build + deploy to `/client/` | Not started. slashcheer-irc serves `web/dist/client/` when `web_client_enabled = true`; plan: this repo's CI publishes the wasm bundle as a release asset and slashcheer-irc's deploy pulls a pinned version into that directory. |

## Blocker inventory (`data` + app)

Counts are files that use each API (data/src + src).

| Area | Native API | Web replacement |
|---|---|---|
| Config file | `tokio::fs`/`std::fs` (17 + 11 files), `dirs-next`, `xdg`, `walkdir` | Config served by the Worker (`/api/client-config`, generated from the user's account) and user edits kept in `localStorage`. Behind a small `data::platform::fs` shim so call sites barely change. |
| History / logs | files + `rusqlite` (bundled) | IndexedDB, or `rusqlite` on `sqlite-wasm-rs` with OPFS (already appears in the dep graph, worth trying first). Server-side `CHATHISTORY` (enabled on Ergo) makes local history optional for v1. |
| Async runtime | `tokio::spawn` (4), `tokio::time` (15), `std::time::Instant` (5) | `wasm_bindgen_futures::spawn_local`, `wasmtimer`/`web-time` (both already in the graph via iced). |
| HTTP | `reqwest` (9 files: previews, uploads, version check) | reqwest's wasm backend (fetch) for previews; drop the version check and anything needing CORS-free access. |
| Secrets | `keyring-core` | Not needed: the web build has no IRC passwords. Compile it out. |
| Processes | `tokio::process`/`std::process` (3) | Compile out (external commands, opening files). |
| Desktop integration | `notify-rust`, `rodio`, `rfd`, `open`, `netwatcher`, `display-info`, `interprocess`/`ipc` | Web Notifications API, `<audio>`, `<input type=file>`, `window.open`; drop netwatcher/IPC/display-info. |
| Fonts | bundled in `fonts/` | Same files, loaded via `iced::font::load` at startup. |
| DCC file transfer | TCP listeners | Unsupported in browsers; hide the UI. |

## Plan

1. **`data` compiles on wasm.** Split `tokio` features by target, add a
   `platform` module (fs/time/spawn shims), gate desktop-only modules behind
   `cfg(not(target_arch = "wasm32"))`. Keep every change `cfg`-scoped.
2. **App boots in the browser.** `halloy` crate builds for wasm with iced's
   web backend (`iced` `webgl`/`wgpu` features), using a hard-coded server
   config pointing at `wss://<same host>/webirc`. Build with `wasm-bindgen`
   (or `trunk`) into a static bundle.
3. **Persistence.** Settings and themes in `localStorage`, buffers/history
   via sqlite-wasm (OPFS) or IndexedDB, plus server `CHATHISTORY`.
4. **Integration polish.** `/api/client-config` from the Worker (network
   name, account, channels), Web Notifications, link previews via fetch,
   file uploads, mobile/touch checks, a CI job that publishes the wasm
   bundle, then flip `web_client_enabled = true` in slashcheer-irc.

Rough size: step 1 is days, step 2 is the riskiest (iced web rendering,
text input/IME, clipboard), steps 3–4 are a few weeks of polish.

## Quick win for desktop Halloy (independent of the web port)

Desktop Halloy already speaks IRC-over-WebSocket (`use_websocket`). A small
fork change, a config option for extra websocket request headers (e.g.
`cf-access-token` from `cloudflared access token`), would let desktop users
connect straight to `wss://<host>/webirc` without `scripts/irc-bridge.sh`.

## Running the smoke test

The smoke page needs something speaking IRC-over-WebSocket at `/webirc` on
the same origin. slashcheer-irc's `worker/test/smoke-gateway.ts` serves this page and
relays to a local Ergo through the real login gate:

```bash
# 1. build the smoke bundle (this repo)
cd web/smoke
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/halloy_web_smoke.wasm

# 2. run Ergo with jwt-auth and an account "alice", then the gateway (slashcheer-irc)
#    see worker/test/gate.integration.ts there for the Ergo setup
cd path/to/slashcheer-irc/worker && ERGO_JWT_KEY=... npm run smoke-gateway -- path/to/halloy/web/smoke

# 3. open http://127.0.0.1:18097/ (or drive it with Playwright) and read window.result
```

`wasm-bindgen` CLI must match the crate version (0.2.108):
`cargo install wasm-bindgen-cli --version 0.2.108 --locked`.
