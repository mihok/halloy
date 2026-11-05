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
the Access cookie automatically on the same-origin `wss://<host>/web/irc`
upgrade, so the web build needs **no credentials, SASL, or login UI**: it
connects with any nickname and the server makes it the user's account. The
only client-side work is a transport.

## Status

| Piece | State |
|---|---|
| `irc` crate on `wasm32` | **Done.** `irc/src/connection_web.rs` is a browser `WebSocket` transport with the same API as the native `connection.rs` (selected in `irc/src/lib.rs` with `#[cfg(target_arch = "wasm32")]`). Proxies/DCC/custom certs return `Error::Unsupported`. Native build and tests unchanged. |
| End-to-end check | **Done.** `web/smoke/` (`smoke`): Halloy's `irc::Connection` + `irc::Codec` compiled to wasm, run in headless Chromium, through the real `IrcGate` to Ergo 2.19.1. Logged in as the bound account with no credentials, nick forced, `+Z` secure, real IP forwarded. |
| iced / wgpu / winit on wasm | **Working.** Renders through wgpu on WebGL2 (verified with software WebGL in headless Chromium). |
| `data` crate on wasm | **Compiles, clippy-clean.** Native workspace (app included) builds, clippy-clean, 174 data tests pass. The browser shims are checked at runtime in headless Chromium by `web/smoke` (`platform_check`: in-memory fs, timers, tasks: 11/11 pass). See "data crate: how it's ported" below. |
| `halloy` app crate on wasm | **Boots and chats.** Built with `cargo build --release --target wasm32-unknown-unknown -p halloy` + `wasm-bindgen`; `web/app/index.html` hosts it. Verified end to end in headless Chromium through the slashcheer login gate to Ergo 2.19.1: connects as the gate's account with no credentials, auto-joins a channel, shows another user's messages and nick list, and a message typed in Halloy reaches the other user. No page errors. Desktop build, clippy and tests unchanged. |
| Persistence | **Done.** Config, drafts, dashboard layout, window state and chat history survive reloads (IndexedDB). Verified in Chromium with a persistent profile: history and layout restored with the server unreachable; an unsent draft restored after reload. |
| Build + deploy to `/web/` | **Done.** `web/build.sh` builds the size-optimized bundle (`--profile web`: opt-level z, fat LTO, stripped; 19.2 MB wasm, 7.8 MB gzipped, under Cloudflare's 25 MiB per-asset limit) into `web/dist/` plus `halloy-web.tar.gz`. The `web` workflow publishes it as release `web-<commit>` on every push to `web-port`; slashcheer-irc pins a release (tag + sha256) in `web/halloy.lock` and serves it at `https://<host>/web/`, filling in the `halloy-*` meta tags. The websocket is `/web/irc`. |

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

Fixed while booting the app (all browser-only):

- `platform::fs::sync`: `std::fs`-shaped access to the same in-memory
  filesystem (`std::fs` itself on desktop); call sites that used `std::fs`
  or `Path::try_exists()` for config, drafts, dashboard and file hosts use
  it.
- `environment::{config,data,cache}_dir()` are `/halloy/{config,data,cache}`
  in the virtual filesystem (dirs-next has no answer in a browser).
- `data::time::Posix` uses `web_time::SystemTime` (std's panics).
- The history worker and search worker run as async tasks polling their
  channel instead of threads; legacy (pre-SQLite) imports never happen;
  the history database is in-memory for the session.

## App crate: how it's ported

- `src/web.rs`: the browser entry point. Installs a console panic hook and
  logging, seeds `config.toml` in the virtual filesystem pointing at the
  page's own host (`wss://<host>/web/irc`), then calls the same `run()` the
  desktop `main()` uses. The page can set `<meta name="halloy-network">`,
  `halloy-websocket-path` and `halloy-channels` (auto-join).
- Browser twins selected with `#[cfg_attr(target_arch = "wasm32", path = ...)]`
  so upstream files stay untouched: `audio_web.rs` (`<audio>` + Blob URL),
  `notification/toast_web.rs` (Web Notifications), `system/network_web.rs`
  (online/offline events).
- `web/rfd-web`: a stand-in for the `rfd` file-dialog crate, added as
  `extern crate rfd_web as rfd` on wasm so `rfd::AsyncFileDialog` call
  sites compile unchanged. Picking files opens the browser picker and copies
  the files into the virtual filesystem; saving returns `None` for now.
- Theme follows `prefers-color-scheme` (matchMedia) instead of mundy;
  `open_url` opens http(s) links in a new tab; deep-link IPC, `/exec`,
  uptime and iced's system-information request are unavailable.
- Subscription recipes use iced's target-aware `BoxStream`/`boxed_stream`;
  `std::time::Instant` → `iced::time::Instant`; `tokio::time`/`fs`/`spawn`
  go through `data::platform`; the logs pane gets records without a thread
  and file logging becomes console logging.
- Desktop `main()` is split: its setup stays, and the font/settings/daemon
  launch moved into `run()`, shared with the browser.

## Persistence: how it works

- `data::platform::fs::persist` mirrors the in-memory filesystem's
  `/halloy/config` and `/halloy/data` to an IndexedDB store (`halloy` /
  `files`, key = path, value = bytes). `init()` (called first thing in
  `src/web.rs`) loads every saved file; after that, every mutating
  filesystem call marks its path dirty and a 400 ms debounce writes the
  current contents (or a delete) in one transaction. `pagehide` flushes
  too. `/halloy/cache` (previews, downloads) is not persisted.
- History: SQLite's OPFS backend (`sqlite-wasm-vfs` sahpool) needs a
  dedicated Worker and Halloy runs on the main thread, so the history
  database stays in memory and the history worker snapshots it
  (`rusqlite` serialize) to `/halloy/data/history.sqlite3` at most 5 s after
  a change and on exit; startup deserializes it. Worst case after a crash
  or closed tab: the last ~5 s of history, which the server's CHATHISTORY
  backfills on the next join.
- History search opens a read-only copy of the latest snapshot per search,
  so results can trail the live view by up to 5 s.
- Drafts are saved by Halloy's own logic 10 s after the first edit.

## Known gaps (next)

- **Snapshot cost.** Each history snapshot serializes the whole database;
  fine for a few MB, worth revisiting (incremental saves, or moving the
  history worker into a Web Worker with OPFS) if histories get large.
- **Storage limits.** IndexedDB quota is browser-managed; Halloy doesn't yet
  request persistent storage (`navigator.storage.persist()`) or report
  quota errors in the UI.
- **Bundle size.** 19.2 MB wasm with the `web` profile (7.8 MB gzipped). Next:
  `wasm-opt -Oz` in CI, drop unused fonts/sounds for web.
- **Downloads.** `save_file` (save image, save theme, DCC) returns `None`;
  map it to browser downloads.
- **Clipboard, IME, touch.** Not yet exercised beyond basic typing.
- **WebGPU.** Only WebGL2 was exercised (headless Chromium has no WebGPU).

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
2. ~~**App boots in the browser.**~~ Done (see above).
3. ~~**Persistence.**~~ Done (see above). Originally: settings and themes in `localStorage`, buffers/history
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
connect straight to `wss://<host>/web/irc` without `the `slashcheer` connector`.

## Building and running the web app

```bash
cargo build --release --target wasm32-unknown-unknown -p halloy
wasm-bindgen --target web --out-dir web/app/pkg \
    target/wasm32-unknown-unknown/release/halloy.wasm
# serve web/app/ from the same origin as /web/irc. For a local run against
# Ergo through the login gate, slashcheer-irc's worker/test/smoke-gateway.ts
# serves a static directory and relays /web/irc:
cd path/to/slashcheer-irc/worker && \
    ERGO_JWT_KEY=... GATE_ACCOUNT=alice npm run smoke-gateway -- path/to/halloy/web/app
# open http://127.0.0.1:18097/
```

`web/app/index.html` is the host page: it mounts the app into `#iced` and
carries the `halloy-*` meta tags.

## Running the smoke test

The smoke page needs something speaking IRC-over-WebSocket at `/web/irc` on
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
