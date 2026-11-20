//! HTTP opening selects a typed control or secondary request before starting any connection task.

mod confirmation;
mod control;
mod request;

use afd_auth::principal::Runner;
use afd_runner::capacity::authority::Decision;
use axum::{body::Body, extract::Request, response::Response};

use crate::{Gateway, Operation, Result, error::ErrorKind};
use control::ControlOpening;
use request::StreamRequest;

/// Initial requests authenticate once; secondary requests must prove their existing connection binding.
pub(super) async fn open(gateway: &Gateway, runner: Runner, request: Request) -> Result<Response> {
    let revision = crate::stream_headers::revision(request.headers())?;
    match StreamRequest::try_from(request)? {
        StreamRequest::Control(request) => {
            // Track preparation too: drain must not finish while an opening can still acquire ownership.
            let preparation = gateway.inner.tasks.token();
            let (opening, body) = ControlOpening::new(gateway, runner, request, revision)?;
            let opening = opening.prepare().await?;
            gateway.inner.tasks.spawn(async move {
                let _preparation = preparation;
                if let Err(error) = opening.run().await {
                    Gateway::observe(&error, Operation::Opening);
                }
            });
            response(body, revision)
        }
        StreamRequest::Secondary(request) => response(request.attach().await?, revision),
    }
}

/// Framing supplies its own finite message bounds; HTTP must keep both body directions open.
/// The answer names the revision the runner asked for, which this daemon speaks.
fn response(body: Body, revision: af_wire::protocol::Revision) -> Result<Response> {
    Ok(http::Response::builder()
        .header(http::header::CONTENT_TYPE, "application/octet-stream")
        .header(af_wire::protocol::VERSION_HEADER, revision.as_str())
        .body(body)?)
}

/// Durable refusals remain distinct from an unavailable database.
pub(crate) fn allowed<T>(decision: Decision<T>) -> Result<T> {
    match decision {
        Decision::Allowed(value) => Ok(value),
        Decision::Refused(refusal) => Err(ErrorKind::Authority { refusal }.into()),
    }
}

/// One source defines the heartbeat cadence advertised in welcome and policy replies.
pub(super) fn interval_millis(runners: &afd_runner::Runners) -> Result<u32> {
    Ok(u32::try_from(runners.heartbeat_interval().as_millis())?)
}
