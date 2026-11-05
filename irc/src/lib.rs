#![allow(clippy::large_enum_variant)]

pub use tokio_util::codec::BytesCodec;

pub use self::codec::{Codec, CodecLog};
pub use self::connection::Connection;

pub mod codec;
#[cfg(not(target_arch = "wasm32"))]
pub mod connection;
// slashcheer fork: browser WebSocket transport with the same API.
#[cfg(target_arch = "wasm32")]
#[path = "connection_web.rs"]
pub mod connection;
pub use proto;
