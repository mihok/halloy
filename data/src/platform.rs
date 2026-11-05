//! Platform shims for the web port (slashcheer fork, see web/PORTING.md).
//!
//! Native builds re-export the real APIs, so desktop behavior is unchanged.
//! On `wasm32` they are replaced with browser-compatible implementations
//! that keep the same signatures, so call sites only change their `use`.

pub mod fs;
pub mod http;
pub mod time;

/// Spawn a future on the current runtime: tokio on desktop, the browser's
/// microtask queue on the web.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Run blocking work off the async executor: tokio's blocking pool on
/// desktop; inline in the browser (there are no threads).
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_blocking<F, R>(
    f: F,
) -> impl std::future::Future<Output = Result<R, JoinError>>
where
    F: FnOnce() -> R + 'static,
    R: 'static,
{
    std::future::ready(Ok(f()))
}

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::task::JoinError;

/// Uninhabited in the browser: inline work can't be cancelled or panic
/// separately from the caller.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub enum JoinError {}

#[cfg(target_arch = "wasm32")]
impl std::fmt::Display for JoinError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

#[cfg(target_arch = "wasm32")]
impl std::error::Error for JoinError {}

/// A spawned task that can be aborted: tokio's `JoinHandle` on desktop, an
/// abortable `spawn_local` future in the browser.
#[cfg(not(target_arch = "wasm32"))]
pub type TaskHandle = tokio::task::JoinHandle<()>;

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct TaskHandle(futures::future::AbortHandle);

#[cfg(target_arch = "wasm32")]
impl TaskHandle {
    pub fn abort(&self) {
        self.0.abort();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_abortable<F>(future: F) -> TaskHandle
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future)
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_abortable<F>(future: F) -> TaskHandle
where
    F: std::future::Future<Output = ()> + 'static,
{
    let (future, handle) = futures::future::abortable(future);
    wasm_bindgen_futures::spawn_local(async move {
        let _ = future.await;
    });
    TaskHandle(handle)
}
