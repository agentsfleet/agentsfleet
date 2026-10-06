//! Which runner this is, as the daemon names it, and the span every lease's
//! work runs under.
//!
//! The daemon's own row is read once from `GET /v1/runners/me` and kept, so
//! each span names the runner that opened it and a farm of runners is told
//! apart in the traces, each joining its daemon record. A daemon that cannot
//! say yet costs the span its runner fields and nothing else: no lease waits
//! on telemetry, and the next lease asks again.

use afd_observability::semconv::{
    ATTR_AGENT_ID, ATTR_EVENT_ID, ATTR_LEASE_ID, ATTR_RUNNER_HOST, ATTR_RUNNER_ID,
    RUNNER_SCOPE_NAME, SPAN_RUNNER_LEASE,
};
use afd_wire::lease::LeasePayload;
use afd_wire::runner::SelfResponse;
use tokio::sync::OnceCell;
use tracing::Span;

use crate::client::ControlPlane;
use crate::error::Result;

/// The log line a failed read of this runner's row writes.
const EVENT_IDENTITY_FAILED: &str = "runner_identity_failed";

/// This runner, as the daemon names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    /// The daemon's id for the runner's row.
    pub(crate) runner_id: Box<str>,
    /// The host the runner reported itself on.
    pub(crate) host: Box<str>,
}

/// This runner's identity, read once and then shared without a lock.
#[derive(Debug, Default)]
pub(crate) struct Whoami(OnceCell<Identity>);

impl Whoami {
    /// This runner's identity; `None` while the daemon cannot say, which the
    /// log records.
    pub(crate) async fn get(&self, plane: &ControlPlane) -> Option<&Identity> {
        match self.0.get_or_try_init(|| read(plane)).await {
            Ok(identity) => Some(identity),
            Err(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_IDENTITY_FAILED;
                tracing::warn!(error_code = code, event);
                None
            }
        }
    }
}

/// Reads this runner's row.
async fn read(plane: &ControlPlane) -> Result<Identity> {
    let body = plane.me().await?;
    let me: SelfResponse<'_> = body.decode()?;
    Ok(Identity {
        runner_id: me.id.into(),
        host: me.host_id.into(),
    })
}

/// The span `lease`'s work runs under, naming `identity`'s runner when known.
///
/// The root of the lease's own trace: no trace context crosses the runner
/// protocol, so the event identifier is what joins it to the daemon's
/// `fleet.delivery` span for the same event. That span carries no lease
/// identifier, so a redelivered event joins every lease that ran it; the
/// lease identifier on this span tells those runs apart.
pub(crate) fn lease_span(identity: Option<&Identity>, lease: &LeasePayload<'_>) -> Span {
    let runner_id = identity.map(|known| &*known.runner_id);
    let host = identity.map(|known| &*known.host);
    let lease_id = lease.lease_id.as_ref();
    let event_id = lease.event.event_id.as_ref();
    let fleet_id = lease.event.fleet_id.as_ref();
    tracing::info_span!(
        target: RUNNER_SCOPE_NAME,
        SPAN_RUNNER_LEASE,
        { ATTR_RUNNER_ID } = runner_id,
        { ATTR_RUNNER_HOST } = host,
        { ATTR_LEASE_ID } = lease_id,
        { ATTR_EVENT_ID } = event_id,
        { ATTR_AGENT_ID } = fleet_id,
    )
}

#[cfg(test)]
#[path = "identity/tests.rs"]
mod tests;
