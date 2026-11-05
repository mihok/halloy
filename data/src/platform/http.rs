//! HTTP differences between desktop reqwest and reqwest's browser (fetch)
//! backend.

use bytes::Bytes;
use reqwest::header::HeaderMap;

/// A response read chunk by chunk. Desktop reqwest has `Response::chunk()`;
/// the browser backend only offers a byte stream, so wrap both behind the
/// same three methods the callers use.
pub struct Chunked {
    #[cfg(not(target_arch = "wasm32"))]
    inner: reqwest::Response,
    #[cfg(target_arch = "wasm32")]
    headers: HeaderMap,
    #[cfg(target_arch = "wasm32")]
    content_length: Option<u64>,
    #[cfg(target_arch = "wasm32")]
    stream:
        std::pin::Pin<Box<dyn futures::Stream<Item = reqwest::Result<Bytes>>>>,
}

impl From<reqwest::Response> for Chunked {
    #[cfg(not(target_arch = "wasm32"))]
    fn from(inner: reqwest::Response) -> Self {
        Self { inner }
    }

    #[cfg(target_arch = "wasm32")]
    fn from(resp: reqwest::Response) -> Self {
        Self {
            headers: resp.headers().clone(),
            content_length: resp.content_length(),
            stream: Box::pin(resp.bytes_stream()),
        }
    }
}

impl Chunked {
    pub fn headers(&self) -> &HeaderMap {
        #[cfg(not(target_arch = "wasm32"))]
        return self.inner.headers();
        #[cfg(target_arch = "wasm32")]
        return &self.headers;
    }

    pub fn content_length(&self) -> Option<u64> {
        #[cfg(not(target_arch = "wasm32"))]
        return self.inner.content_length();
        #[cfg(target_arch = "wasm32")]
        return self.content_length;
    }

    pub async fn chunk(&mut self) -> reqwest::Result<Option<Bytes>> {
        #[cfg(not(target_arch = "wasm32"))]
        return self.inner.chunk().await;
        #[cfg(target_arch = "wasm32")]
        {
            use futures::StreamExt;
            self.stream.next().await.transpose()
        }
    }
}

/// TLS client identity (SASL EXTERNAL uploads). The browser can't present
/// client certificates, so the type is uninhabited there.
#[cfg(not(target_arch = "wasm32"))]
pub type Identity = reqwest::Identity;
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub enum Identity {}
