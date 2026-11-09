//! The initial read owns its permit and proposal while Postgres remains unchanged.

use super::super::{Hello, SessionOpening, StreamAttachment, io::StreamIo};
use super::{allowed, request::ControlRequest};
use crate::{
    ConnectionContext, Gateway, Operation, Result,
    error::{ErrorKind, within},
};
use af_core::{id::Uuid7, timing::STREAM_OPEN_TIMEOUT};
use af_stream::OwnedFrame;
use af_wire::protocol::{StreamKind, Up};
use afd_auth::{directory::Digest, principal::Runner};
use afd_sse::StreamPermit;
use axum::body::Body;
use garde::Validate as _;
use std::sync::Arc;
use tokio::time::Instant;

pub(super) struct ControlOpening {
    gateway: Gateway,
    context: Arc<ConnectionContext>,
    runner: Runner,
    previous: Option<Uuid7>,
    digest: Digest,
    io: StreamIo,
    permit: StreamPermit,
    deadline: Instant,
}

impl TryFrom<&OwnedFrame> for Hello {
    type Error = crate::Error;

    fn try_from(frame: &OwnedFrame) -> Result<Self> {
        let message: Up<'_> = frame.decode()?;
        message.validate()?;
        match message {
            Up::Hello {
                request_id,
                accepted_kinds,
                running,
                credits,
                unconfirmed_connection_id,
            } => Ok(Self {
                request_id,
                accepted: accepted_kinds,
                credits,
                running: running
                    .iter()
                    .map(|id| Uuid7::parse(id))
                    .collect::<af_core::error::Result<_>>()?,
                unconfirmed: unconfirmed_connection_id
                    .as_deref()
                    .map(Uuid7::parse)
                    .transpose()?,
            }),
            _ => Err(ErrorKind::Protocol.into()),
        }
    }
}

impl ControlOpening {
    /// A session consumes one shared permit; secondary streams consume none.
    pub(super) fn new(
        gateway: &Gateway,
        runner: Runner,
        request: ControlRequest,
    ) -> Result<(Self, Body)> {
        if gateway.inner.registry.is_draining() {
            return Err(ErrorKind::Closed.into());
        }
        let limit = &gateway.inner.dependencies.stream_limit;
        let permit = limit.admit().ok_or(ErrorKind::Capacity {
            carrying: limit.live(),
            capacity: limit.capacity(),
        })?;
        let (attachment, body) = StreamAttachment::new(StreamKind::Control, request.body);
        let io = attachment.into_io(
            gateway.inner.input_budget.clone(),
            request.context.native.stop.clone(),
        );
        Ok((
            Self {
                gateway: gateway.clone(),
                context: request.context,
                runner,
                previous: request.previous,
                digest: request.digest,
                io,
                permit,
                deadline: Instant::now() + STREAM_OPEN_TIMEOUT,
            },
            body,
        ))
    }

    /// Reading hello and proposing ownership share one absolute opening deadline.
    pub(super) async fn prepare(mut self) -> Result<SessionOpening> {
        let hello = self.read_hello().await?;
        let prepared = within(
            &self.context.native.stop,
            self.deadline,
            Operation::Opening,
            async {
                allowed(
                    self.gateway
                        .inner
                        .dependencies
                        .runners
                        .prepare_connection(
                            self.runner.id(),
                            self.digest,
                            self.previous.as_ref(),
                            hello.unconfirmed.as_ref(),
                            self.gateway.inner.dependencies.wall_clock.now(),
                        )
                        .await?,
                )
            },
        )
        .await?;
        Ok(SessionOpening {
            gateway: self.gateway,
            context: self.context,
            runner: self.runner,
            hello,
            prepared,
            io: self.io,
            permit: self.permit,
            deadline: self.deadline,
        })
    }

    /// Conversion owns the identifiers; returning drops and zeroes the original charged frame.
    async fn read_hello(&mut self) -> Result<Hello> {
        let frame = within(
            &self.context.native.stop,
            self.deadline,
            Operation::Opening,
            async {
                self.io
                    .reader
                    .next()
                    .await?
                    .ok_or_else(|| ErrorKind::Closed.into())
            },
        )
        .await?;
        Hello::try_from(&frame)
    }
}
