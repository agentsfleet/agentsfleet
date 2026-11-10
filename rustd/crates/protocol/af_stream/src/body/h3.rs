use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Body, Frame};
use tokio_quiche::http3::driver::{InboundFrame, InboundFrameStream};

use crate::{Error, Result, error::ErrorKind};

pub(super) struct H3Body(pub(super) Option<InboundFrameStream>);

impl Body for H3Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>>>> {
        let Some(input) = self.0.as_mut() else {
            return Poll::Ready(None);
        };
        match std::task::ready!(input.poll_recv(cx)) {
            Some(InboundFrame::Body(bytes, fin)) => {
                if fin {
                    self.0 = None;
                }
                Poll::Ready(Some(Ok(Frame::data(bytes.freeze()))))
            }
            Some(InboundFrame::Datagram(_)) => {
                self.0 = None;
                Poll::Ready(Some(Err(ErrorKind::Protocol.into())))
            }
            None => {
                self.0 = None;
                Poll::Ready(None)
            }
        }
    }
}
