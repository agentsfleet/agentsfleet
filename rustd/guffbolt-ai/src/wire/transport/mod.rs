//! The transport seam: [`Send`] carries one request and answers its streamed
//! response, in `http`'s types and never rig's, so a rig upgrade does not
//! break an embedder's transport.
//!
//! Under rig, every send goes through [`HttpTransport`], which reads the body
//! as it arrives and stops it at the byte bound the embedder set, if one.
//! The default transport, behind feature `reqwest`, follows no redirect: the
//! key goes only to the host the route names.

use std::error::Error as StdError;
use std::fmt::{self, Debug};
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

use bytes::{Bytes, BytesMut};
use futures_util::stream::BoxStream;
use futures_util::{TryStreamExt as _, future::BoxFuture};
use http::{Request, Response};
use rig_core::http_client::{
    self as rig_http, HttpClientExt, LazyBody, MultipartForm, StreamingResponse,
};
use rig_core::wasm_compat::WasmCompatSend;

use crate::Error;
use crate::error::{ErrorKind, Result};

#[cfg(feature = "reqwest")]
mod reqwest;
#[cfg(feature = "reqwest")]
pub use self::reqwest::{IsRefused, Reqwest};

/// A response body: its bytes as they arrive.
pub type Body = BoxStream<'static, Result<Bytes, SendError>>;

/// What a send answers: the response head, with its body still to be read.
pub type Sent = BoxFuture<'static, Result<Response<Body>, SendError>>;

/// The failure a send, or a body read, carries as its cause.
pub type Cause = Box<dyn StdError + core::marker::Send + Sync>;

/// One physical send. It sends once: retrying is the caller's
/// ([`Retry`](super::Retry)).
pub trait Send: core::marker::Send + Sync {
    /// Sends `request` and answers the response without collecting its body.
    fn send(&self, request: Request<Bytes>) -> Sent;
}

/// A shared transport sends as the one it shares.
impl<T: Send + ?Sized> Send for Arc<T> {
    fn send(&self, request: Request<Bytes>) -> Sent {
        (**self).send(request)
    }
}

/// Why a send, or a read of its body, failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum SendError {
    /// The embedder's guard refused the destination. No retry or reopening
    /// repeats it: it would be refused the same way.
    Refused(Cause),
    /// No answer came: the connection was refused or timed out. Another send
    /// may be answered.
    Unanswered(Cause),
    /// Anything else, a body cut mid-read included.
    Failed(Cause),
}

impl SendError {
    /// Whether the embedder's guard refused the destination.
    #[must_use]
    pub const fn is_refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }

    /// Whether another send may be answered.
    #[must_use]
    pub const fn is_unanswered(&self) -> bool {
        matches!(self, Self::Unanswered(_))
    }

    /// The failure as rig reads it: a refusal is permanent, anything else may
    /// pass on another opening.
    fn into_rig(self) -> rig_http::Error {
        if self.is_refused() {
            rig_http::Error::permanent(self)
        } else {
            rig_http::Error::transient(self)
        }
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Refused(_) => "the transport refused the destination",
            Self::Unanswered(_) => "the request was not answered",
            Self::Failed(_) => "the transport failed",
        })
    }
}

impl StdError for SendError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Refused(cause) | Self::Unanswered(cause) | Self::Failed(cause) => Some(&**cause),
        }
    }
}

#[cfg(test)]
mod tests;

/// The rig HTTP adapter over a [`Send`]. Diagnostics never include the
/// request or response body.
#[derive(Clone)]
pub(crate) struct HttpTransport {
    physical: Arc<dyn Send>,
    status: Arc<AtomicU16>,
    bound: Option<usize>,
}

impl Debug for HttpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTransport")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Wraps the caller's physical sender, reading every reply whole.
    #[must_use]
    pub(crate) fn new(physical: Arc<dyn Send>) -> Self {
        Self {
            physical,
            status: Arc::new(AtomicU16::new(0)),
            bound: None,
        }
    }

    /// The same adapter, refusing a reply past `bound` bytes, when there is one.
    #[must_use]
    pub(crate) const fn with_bound(mut self, bound: Option<usize>) -> Self {
        self.bound = bound;
        self
    }

    /// Observed HTTP status; zero means no response head arrived.
    #[must_use]
    pub(crate) fn status(&self) -> u16 {
        self.status.load(Ordering::Relaxed)
    }

    async fn streaming(
        &self,
        request: Request<Bytes>,
    ) -> Result<StreamingResponse, Box<rig_http::Error>> {
        let response = self
            .physical
            .send(request)
            .await
            .map_err(|failure| Box::new(failure.into_rig()))?;
        self.status
            .store(response.status().as_u16(), Ordering::Relaxed);
        let (parts, body) = response.into_parts();
        let body = body.map_err(SendError::into_rig);
        let body = futures_util::stream::try_unfold(
            (body, 0_usize, self.bound),
            async |(mut body, read, bound)| {
                let Some(chunk) = body.try_next().await? else {
                    return Ok(None);
                };
                let read = read.saturating_add(chunk.len());
                if bound.is_some_and(|bound| read > bound) {
                    return Err(rig_http::Error::permanent(Error::from(
                        ErrorKind::ReplyTooLarge,
                    )));
                }
                Ok(Some((chunk, (body, read, bound))))
            },
        );
        Ok(Response::from_parts(parts, Box::pin(body)))
    }
}

impl HttpClientExt for HttpTransport {
    fn send(
        &self,
        req: Request<Bytes>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody>>> + WasmCompatSend + 'static {
        let transport = self.clone();
        async move {
            let response = transport.streaming(req).await.map_err(|source| *source)?;
            let (parts, body) = response.into_parts();
            let read: LazyBody = Box::pin(async move {
                body.try_fold(BytesMut::new(), async move |mut body, chunk| {
                    body.extend_from_slice(&chunk);
                    Ok(body)
                })
                .await
                .map(BytesMut::freeze)
            });
            Ok(Response::from_parts(parts, read))
        }
    }

    fn send_multipart(
        &self,
        _request: Request<MultipartForm>,
    ) -> impl Future<Output = rig_http::Result<Response<LazyBody>>> + WasmCompatSend + 'static {
        std::future::ready(Err(rig_http::Error::permanent(Error::from(
            ErrorKind::Multipart,
        ))))
    }

    fn send_streaming(
        &self,
        req: Request<Bytes>,
    ) -> impl Future<Output = rig_http::Result<StreamingResponse>> + WasmCompatSend {
        let transport = self.clone();
        async move { transport.streaming(req).await.map_err(|source| *source) }
    }
}
