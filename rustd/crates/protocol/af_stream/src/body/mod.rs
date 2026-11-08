//! One bounded HTTP body at the transport boundary.

mod h3;

use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body::{Body, Frame};
use http_body_util::{BodyDataStream, BodyExt, StreamBody, combinators::UnsyncBoxBody};
use tokio_quiche::http3::driver::InboundFrameStream;
use tokio_util::{
    io::{ReaderStream, StreamReader},
    sync::{CancellationToken, WaitForCancellationFutureOwned},
};

use crate::{Error, Result, error::ErrorKind};

/// Upload or download body, boxed once where native transports meet.
pub struct Outgoing(UnsyncBoxBody<Bytes, Error>);

impl fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Outgoing").finish_non_exhaustive()
    }
}

impl Outgoing {
    /// Retain a foreign body's original failure as its source.
    pub fn new<B>(body: B) -> Self
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        Self(
            body.map_err(|source| {
                Error::from(ErrorKind::Body {
                    source: Box::new(source),
                })
            })
            .boxed_unsync(),
        )
    }

    /// A fixed-size pipe applies backpressure before another chunk is allocated.
    #[must_use]
    pub fn pipe() -> (tokio::io::DuplexStream, Self) {
        let (writer, reader) = tokio::io::duplex(crate::CHUNK_BYTES);
        let stream = ReaderStream::with_capacity(reader, crate::CHUNK_BYTES)
            .map_ok(Frame::data)
            .map_err(Error::from);
        (writer, Self(StreamBody::new(stream).boxed_unsync()))
    }

    pub(crate) async fn data(&mut self) -> Result<Option<Bytes>> {
        let Some(frame) = self.0.frame().await.transpose()? else {
            return Ok(None);
        };
        Ok(Some(
            frame.into_data().map_err(|_trailers| ErrorKind::Protocol)?,
        ))
    }
}

impl Body for Outgoing {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>>>> {
        Pin::new(&mut self.0).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.0.size_hint()
    }
}

/// Native response/request body with cancellation registered once per stream.
pub struct Incoming {
    native: Option<Native>,
    cancelled: Pin<Box<WaitForCancellationFutureOwned>>,
}

enum Native {
    H2(hyper::body::Incoming),
    H3(h3::H3Body),
}

impl fmt::Debug for Incoming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incoming")
            .field("finished", &self.native.is_none())
            .finish_non_exhaustive()
    }
}

impl Incoming {
    pub(crate) fn h2(stream: hyper::body::Incoming, stop: CancellationToken) -> Self {
        Self {
            native: Some(Native::H2(stream)),
            cancelled: Box::pin(stop.cancelled_owned()),
        }
    }

    pub(crate) fn h3(stream: InboundFrameStream, stop: CancellationToken) -> Self {
        Self {
            native: Some(Native::H3(h3::H3Body(Some(stream)))),
            cancelled: Box::pin(stop.cancelled_owned()),
        }
    }

    /// Adapt to the bounded application frame reader without collecting the body.
    pub fn reader(self) -> impl tokio::io::AsyncRead + Send + Unpin {
        StreamReader::new(BodyDataStream::new(self).map_err(std::io::Error::other))
    }
}

impl Body for Incoming {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>>>> {
        if self.native.is_none() {
            return Poll::Ready(None);
        }
        if self.cancelled.as_mut().poll(cx).is_ready() {
            self.native = None;
            return Poll::Ready(Some(Err(ErrorKind::Cancelled.into())));
        }
        let Some(native) = self.native.as_mut() else {
            return Poll::Ready(None);
        };
        let answer = std::task::ready!(Pin::new(native).poll_frame(cx));
        if answer.is_none() || matches!(answer, Some(Err(_))) {
            self.native = None;
        }
        Poll::Ready(answer)
    }
}

impl Body for Native {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>>>> {
        match self.get_mut() {
            Self::H2(body) => Pin::new(body)
                .poll_frame(cx)
                .map(|frame| frame.map(|result| result.map_err(Error::from))),
            Self::H3(body) => Pin::new(body).poll_frame(cx),
        }
    }
}
