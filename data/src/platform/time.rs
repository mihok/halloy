//! Timers: tokio on desktop, browser timers (wasmtimer) on the web, where
//! tokio's timer driver doesn't exist.

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::{
    Instant, Interval, MissedTickBehavior, interval, interval_at, sleep,
    sleep_until, timeout,
};
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::Instant;
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::tokio::{
    Interval, MissedTickBehavior, interval, interval_at, sleep, sleep_until,
    timeout,
};

/// Make the interval's next tick complete immediately.
#[cfg(not(target_arch = "wasm32"))]
pub fn reset_immediately(interval: &mut Interval) {
    interval.reset_immediately();
}

/// wasmtimer has no `reset_immediately`; a fresh interval's first tick is
/// immediate, so rebuild it with the same period and behavior.
#[cfg(target_arch = "wasm32")]
pub fn reset_immediately(interval: &mut Interval) {
    let behavior = interval.missed_tick_behavior();
    *interval = wasmtimer::tokio::interval(interval.period());
    interval.set_missed_tick_behavior(behavior);
}
