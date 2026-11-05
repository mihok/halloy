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
| End-to-end check | **Done.** `web/smoke/` (`smoke`): Halloy's `irc::Connection` + `irc::Codec` compiled to wasm, run in headless Chromium, through the real `IrcGate` to Ergo 2.19.1. Logged in as the bound account with no credentials, nick forced, `+Z` secure, real IP forwarded. |
| iced / wgpu / winit on wasm | Compile (seen during `cargo check -p data --target wasm32-unknown-unknown`). Rendering not yet tried. |
| `data` crate on wasm | **Compiles, clippy-clean.** Native workspace (app included) builds, clippy-clean, 174 data tests pass. The browser shims are checked at runtime in headless Chromium by `web/smoke` (`platform_check`: in-memory fs, timers, tasks: 11/11 pass). See "data crate: how it's ported" below. |
| `halloy` app crate on wasm | **Next.** |
| Build + deploy to `/client/` | Not started. slashcheer-irc serves `web/dist/client/` when `web_client_enabled = true`; plan: this repo's CI publishes the wasm bundle as a release asset and slashcheer-irc's deploy pulls a pinned version into that directory. |

## data crate: how it's ported

All changes are `cfg(target_arch = "wasm32")`-gated; desktop code paths are
unchanged. New code is in `data/src/platform/`:

| Shim | Desktop | Browser |
|---|---|---|
| `platform::fs` | re-exports `tokio::fs` + `ReadDirStream` | in-memory filesystem with the same async API, including `File`/`OpenOptions` implementing tokio's `AsyncRead`/`AsyncWrite`/`AsyncSeek`, `read_dir`, `metadata`. Not persisted yet. |
| `platform::time` | `tokio::time` | `wasmtimer` timers; `reset_immediately` helper |
| `platform::spawn`, `spawn_blocking`, `spawn_abortable`, `JoinError`, `TaskHandle` | tokio tasks | `spawn_local`; blocking work runs inline |
| `platform::http::Chunked` | wraps `reqwest::Response::chunk()` | reqwest fetch backend `bytes_stream()` |
| `platform::http::Identity` | `reqwest::Identity` | uninhabited (no client certs in browsers) |

Other changes:

- `std::time::Instant` → `web_time::Instant` (identical type on desktop;
  `performance.now()` in the browser, where `std`'s panics).
- `tokio::fs`/`tokio::time` call sites import from `platform` instead.
- Browser-only behavior: HTTP proxies and client certificates return
  errors; `password_command` returns an error (no shell); window position
  validation always passes; uploads send the body whole (fetch can't stream
  request bodies); GIF animations decode inline from the in-memory files.
- Cargo: tokio, tokio-stream, reqwest, iced and display-info are split into
  native and wasm target tables; `getrandom` gets `wasm_js` for `rand`;
  browser iced uses `wgpu` + `webgl` with `tiny-skia` fallback and
  `web-colors`. `web-time` is a normal dependency (it *is* `std::time` on
  desktop).

Known runtime gaps to handle when the app boots:

- `std::fs` still appears in ~11 files (config, dashboard, logs, legacy
  history). It compiles in the browser but fails at runtime; route these
  through `platform::fs` (sync variants) as they come up.
- `SystemTime::now()` panics in the browser; chrono's clock is fine
  (wasmbind). Audit when the app runs.
- `keyring` has no browser backend; the web build has no passwords to
  store, so it should never be reached.
- History storage uses rusqlite (`sqlite-wasm-rs` in the browser) on the
  virtual paths. Needs a real persistence decision (OPFS VFS vs. server
  CHATHISTORY only).

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

1. ~~**`data` compiles on wasm.**~~ Done (see above).
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
# 1. build the smoke bundle (this repo; web/smoke is a workspace member,
#    compiled empty on desktop)
cargo build --release --target wasm32-unknown-unknown -p halloy-web-smoke
wasm-bindgen --target web --out-dir web/smoke/pkg target/wasm32-unknown-unknown/release/halloy_web_smoke.wasm

# 2. run Ergo with jwt-auth and an account "alice", then the gateway (slashcheer-irc)
#    see worker/test/gate.integration.ts there for the Ergo setup
cd path/to/slashcheer-irc/worker && ERGO_JWT_KEY=... npm run smoke-gateway -- path/to/halloy/web/smoke

# 3. open http://127.0.0.1:18097/ (or drive it with Playwright):
#    window.platform = runtime checks of data::platform (fs, timers, tasks)
#    window.result   = IRC lines received through the gate
#    add ?platform-only to skip the IRC part (no gateway/Ergo needed for
#    window.platform; any static server for web/smoke works)
```

`wasm-bindgen` CLI must match the crate version (0.2.108):
`cargo install wasm-bindgen-cli --version 0.2.108 --locked`.
