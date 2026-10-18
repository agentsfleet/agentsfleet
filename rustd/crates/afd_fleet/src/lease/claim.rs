//! One poll selects between independently authorized placement domains.

use afd_core::id::Uuid7;
use afd_wire::lease::{AcceptedKinds, LeaseKind, LeaseResponse, TaggedLease};
use jiff::Timestamp;

use super::{Plane, answer::no_work, pull::Polled};
use crate::{
    Result,
    error::{DETAIL_EVENT_MALFORMED, rejected},
};

impl Plane {
    /// Place one accepted kind, alternating after each committed placement.
    ///
    /// The optimistic choice is rechecked by both issue transactions under
    /// the same host row lock. A lost choice yields no work until a fresh poll.
    ///
    /// # Errors
    /// Preserves either domain's refusal and the host-choice datastore failure.
    pub async fn claim(
        &self,
        drafts: &afd_authoring::Drafts,
        runner: &Uuid7,
        held: &[Uuid7],
        accepted: AcceptedKinds,
        degraded: bool,
        now: Timestamp,
    ) -> Result<String> {
        if degraded {
            return no_work(runner, "the runner's verdict is degraded or unreadable");
        }
        let hosts =
            afd_runner::Runners::new(self.leases.pool().clone(), self.leases.entropy().clone());
        let Some(ticket) = hosts.claim_offer(runner, accepted).await? else {
            return no_work(runner, "the runner is unavailable");
        };
        for kind in ticket.order() {
            match kind {
                LeaseKind::Authoring => {
                    if let Some(lease) = drafts.claim_offered(runner, ticket).await? {
                        return serde_json::to_string(&LeaseResponse {
                            lease: Some(TaggedLease::Authoring(Box::new(lease))),
                            retry_after_ms: None,
                        })
                        .map_err(|_shape| rejected(DETAIL_EVENT_MALFORMED));
                    }
                }
                LeaseKind::Fleet => {
                    if let Polled::Lease(body) =
                        self.poll(runner, held, false, now, Some(ticket)).await?
                    {
                        return Ok(body);
                    }
                }
            }
        }
        no_work(runner, "no eligible accepted work")
    }
}
