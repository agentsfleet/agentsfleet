//! Converting the HTTP request proves all metadata before it can consume a stream slot.

use super::super::StreamAttachment;
use crate::{
    ConnectionContext, Operation, Result,
    error::{ErrorKind, within},
    stream_headers::StreamHeaders,
};
use af_core::{id::Uuid7, timing::STREAM_OPEN_TIMEOUT};
use af_wire::protocol::StreamKind;
use afd_auth::directory::Digest;
use afd_http::auth::AuthenticatedRunnerCredential;
use axum::{body::Body, extract::Request};
use std::sync::Arc;
use tokio::time::Instant;

/// Each variant carries only the proof its operation may use.
pub(super) enum StreamRequest {
    Control(ControlRequest),
    Secondary(SecondaryRequest),
}

pub(super) struct ControlRequest {
    pub(super) context: Arc<ConnectionContext>,
    pub(super) previous: Option<Uuid7>,
    pub(super) digest: Digest,
    pub(super) body: Body,
}

pub(super) struct SecondaryRequest {
    context: Arc<ConnectionContext>,
    headers: StreamHeaders,
    body: Body,
}

impl TryFrom<Request> for StreamRequest {
    type Error = crate::Error;

    fn try_from(request: Request) -> Result<Self> {
        let (parts, body) = request.into_parts();
        let context = parts
            .extensions
            .get::<Arc<ConnectionContext>>()
            .cloned()
            .ok_or(ErrorKind::Binding)?;
        let headers = StreamHeaders::try_from(&parts.headers)?;
        context.reserve(headers.stream_kind())?;
        match headers {
            StreamHeaders::Control { previous } => {
                let digest = parts
                    .extensions
                    .get::<AuthenticatedRunnerCredential>()
                    .ok_or(ErrorKind::Binding)?
                    .digest()
                    .clone();
                Ok(Self::Control(ControlRequest {
                    context,
                    previous,
                    digest,
                    body,
                }))
            }
            headers => Ok(Self::Secondary(SecondaryRequest {
                context,
                headers,
                body,
            })),
        }
    }
}

impl SecondaryRequest {
    /// The server-created binding belongs to this physical connection, not merely the supplied UUID.
    pub(super) async fn attach(self) -> Result<Body> {
        let binding = self.context.binding(&self.headers)?;
        let kind = self.headers.stream_kind();
        if kind == StreamKind::Control {
            return Err(ErrorKind::Binding.into());
        }
        let (attachment, body) = StreamAttachment::new(kind, self.body);
        within(
            &self.context.native.stop,
            Instant::now() + STREAM_OPEN_TIMEOUT,
            Operation::Opening,
            async {
                binding
                    .attachments
                    .send(attachment)
                    .await
                    .map_err(crate::error::closed_send)
            },
        )
        .await?;
        Ok(body)
    }
}
