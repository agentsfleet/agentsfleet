//! The `agentsfleetd` verbs one lease's tools reach, fenced by that lease.
//!
//! The supervisor holds the lease's id and fencing token; a tool holds
//! neither. Every call is one attempt: a schedule create is not idempotent and
//! a line that landed and lost its answer would post twice, so the model, not
//! a retry loop, decides whether to try again.

use afd_core::id::Uuid7;
use afd_wire::message_verb::MessagePosted;
use afr_agent::{ScheduleCall, Unanswered};

use crate::client::ControlPlane;

/// The verbs of one held lease, over the control plane.
#[derive(Debug)]
pub(crate) struct LeaseVerbs<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
    fencing_token: u64,
}

impl<'a> LeaseVerbs<'a> {
    /// The verbs of `lease_id`, fenced by `fencing_token`, through `plane`.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        lease_id: &'a Uuid7,
        fencing_token: u64,
    ) -> Self {
        Self {
            plane,
            lease_id,
            fencing_token,
        }
    }
}

#[async_trait::async_trait]
impl afr_agent::LeaseVerbs for LeaseVerbs<'_> {
    async fn schedules(&self, call: ScheduleCall<'_>) -> Result<String, Unanswered> {
        self.plane
            .schedules(self.lease_id, self.fencing_token, call)
            .await
            .map(|body| body.text())
            .map_err(|failure| unanswered(&failure))
    }

    async fn message(&self, text: &str) -> Result<bool, Unanswered> {
        let body = self
            .plane
            .message(self.lease_id, self.fencing_token, text)
            .await
            .map_err(|failure| unanswered(&failure))?;
        body.decode::<MessagePosted>()
            .map(|posted| posted.delivered)
            .map_err(|failure| unanswered(&failure))
    }
}

/// A failed call as a tool reads it: a refusal keeps its registry code, and
/// anything else is `agentsfleetd` out of reach.
fn unanswered(failure: &crate::Error) -> Unanswered {
    match failure.refusal_status() {
        Some(_status) => Unanswered::Refused(failure.refusal_code()),
        None => Unanswered::Unreachable,
    }
}

#[cfg(test)]
#[path = "verbs/tests.rs"]
mod tests;
