//! Decode inside the charged frame; domain work runs separately from stream reads and pings.

use std::sync::Arc;

use af_wire::protocol::Up;
use garde::Validate as _;

use super::{Message, Request, SessionServices, io::IncomingFrame};
use crate::{Result, error::ErrorKind};

mod authoring;
mod heartbeat;
mod progress;
mod renewal;
mod report;

impl SessionServices {
    #[tracing::instrument(
        name = "runner_session_dispatch",
        level = "debug",
        skip_all,
        fields(stream_kind = incoming.kind.as_str(), request_id = tracing::field::Empty,
            operation = tracing::field::Empty)
    )]
    pub(crate) async fn dispatch(
        self: Arc<Self>,
        incoming: IncomingFrame,
        requests: &Arc<super::requests::RequestTracker>,
    ) -> Result<()> {
        let message: super::Request<'_> = incoming.frame.decode()?;
        message.validate()?;
        if message.stream_kind() != incoming.kind {
            return Err(ErrorKind::Protocol.into());
        }
        let request_id = message.request_id();
        let registration = requests.register(request_id)?;
        let span = tracing::Span::current();
        if let Some(request_id) = request_id {
            span.record("request_id", request_id.get());
        }
        span.record("operation", operation(&message));
        let result = self.handle(message).await;
        drop(span);
        drop(registration);
        result
    }

    async fn handle(&self, message: Request<'_>) -> Result<()> {
        match message {
            Up::Ping { request_id } => self.send(&Message::Pong { request_id }).await?,
            Up::Heartbeat {
                request_id,
                request,
            } => self.heartbeat(request_id, request).await?,
            Up::RenewBatch { request_id, leases } => self.renew(request_id, leases).await?,
            Up::Report {
                request_id,
                request,
            } => self.report(request_id, &request).await?,
            Up::Folds {
                request_id,
                lease_id,
                request,
            } => self.folds(request_id, &lease_id, &request).await?,
            Up::Activity { lease_id, request } => self.progress(&lease_id, &request).await?,
            Up::Ready {
                credits,
                revision,
                observed_offers,
            } => {
                let increased =
                    self.handle
                        .snapshot
                        .lock()
                        .availability(credits, revision, observed_offers)?;
                if increased {
                    self.handle.request_offer();
                }
            }
            Up::AuthoringModel {
                request_id,
                lease_id,
                request,
            } => {
                self.authoring_model(request_id, &lease_id, request.into_owned())
                    .await?;
            }
            Up::AuthoringActivity {
                request_id,
                lease_id,
                request,
            } => {
                self.authoring_activity(request_id, &lease_id, request.into_owned())
                    .await?;
            }
            Up::AuthoringRenew {
                request_id,
                lease_id,
                request,
            } => self.authoring_renew(request_id, &lease_id, request).await?,
            Up::AuthoringReport {
                request_id,
                lease_id,
                request,
            } => {
                self.authoring_report(request_id, &lease_id, request.into_owned())
                    .await?;
            }
            Up::Hello { .. } | Up::Confirm { .. } => return Err(ErrorKind::Protocol.into()),
        }
        Ok(())
    }
}

/// Payload text must never become an operation label in the tracing span.
fn operation(message: &Request<'_>) -> &'static str {
    match message {
        Up::Ping { .. } => "ping",
        Up::Heartbeat { .. } => "heartbeat",
        Up::RenewBatch { .. } => "renewal",
        Up::Report { .. } => "report",
        Up::Folds { .. } => "folds",
        Up::Activity { .. } => "activity",
        Up::Ready { .. } => "ready",
        Up::AuthoringModel { .. } => "authoring_model",
        Up::AuthoringActivity { .. } => "authoring_activity",
        Up::AuthoringRenew { .. } => "authoring_renewal",
        Up::AuthoringReport { .. } => "authoring_report",
        Up::Hello { .. } => "hello",
        Up::Confirm { .. } => "confirm",
    }
}
