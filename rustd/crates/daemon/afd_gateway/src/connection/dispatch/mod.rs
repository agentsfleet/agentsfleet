//! Decode inside the charged frame; domain work runs separately from stream reads and pings.

use std::sync::Arc;

use af_wire::protocol::Up;
use garde::Validate as _;

use super::{ConnectionServices, Message, Request, io::IncomingFrame};
use crate::{Result, error::ErrorKind};

mod authoring;
mod heartbeat;
mod progress;
mod renewal;
mod report;
mod turn_checkpoint;

impl ConnectionServices {
    #[tracing::instrument(
        name = "runner_connection_dispatch",
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
        if matches!(message, Up::TurnCheckpoint { .. }) {
            af_wire::run::TurnMessageSize {
                bytes: incoming.frame.as_bytes().len(),
            }
            .validate()?;
        }
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

    fn availability(
        &self,
        credits: usize,
        revision: std::num::NonZeroU64,
        observed_assignments: u64,
    ) -> Result<()> {
        let increased =
            self.handle
                .snapshot
                .lock()
                .availability(credits, revision, observed_assignments)?;
        if increased {
            self.handle.request_assignment();
        }
        Ok(())
    }

    async fn handle(&self, message: Request<'_>) -> Result<()> {
        match message {
            Up::Ping { request_id } => self.send(&Message::Pong { request_id }).await?,
            Up::Heartbeat {
                request_id,
                request,
            } => self.heartbeat(request_id, request).await?,
            Up::RenewBatch { request_id, runs } => self.renew(request_id, runs).await?,
            Up::Report {
                request_id,
                request,
            } => self.report(request_id, &request).await?,
            Up::Folds {
                request_id,
                run_id,
                request,
            } => self.folds(request_id, &run_id, &request).await?,
            Up::TurnCheckpoint {
                request_id,
                run_id,
                request,
            } => {
                self.turn_checkpoint(request_id, &run_id, request.as_ref())
                    .await?;
            }
            Up::Progress { run_id, request } => self.progress(&run_id, &request).await?,
            Up::Ready {
                credits,
                revision,
                observed_assignments,
            } => {
                self.availability(credits, revision, observed_assignments)?;
            }
            Up::AuthoringModel {
                request_id,
                run_id,
                request,
            } => {
                self.authoring_model(request_id, &run_id, request.into_owned())
                    .await?;
            }
            Up::AuthoringActivity {
                request_id,
                run_id,
                request,
            } => {
                self.authoring_activity(request_id, &run_id, request.into_owned())
                    .await?;
            }
            Up::AuthoringRenew {
                request_id,
                run_id,
                request,
            } => self.authoring_renew(request_id, &run_id, request).await?,
            Up::AuthoringReport {
                request_id,
                run_id,
                request,
            } => {
                self.authoring_report(request_id, &run_id, request.into_owned())
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
        Up::TurnCheckpoint { .. } => "turn_checkpoint",
        Up::Progress { .. } => "progress",
        Up::Ready { .. } => "ready",
        Up::AuthoringModel { .. } => "authoring_model",
        Up::AuthoringActivity { .. } => "authoring_activity",
        Up::AuthoringRenew { .. } => "authoring_renewal",
        Up::AuthoringReport { .. } => "authoring_report",
        Up::Hello { .. } => "hello",
        Up::Confirm { .. } => "confirm",
    }
}
