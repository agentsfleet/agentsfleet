//! One poll selects between independently authorized placement domains.

use af_core::id::Uuid7;
use af_wire::lease::{AcceptedKinds, LeaseKind, LeaseResponse, TaggedLease};
use jiff::Timestamp;

use super::{LeaseService, answer::no_work, pull::Polled};
use super::{OfferRequest, Offered};
use crate::{
    Result,
    error::{DETAIL_EVENT_MALFORMED, rejected},
};

impl LeaseService {
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
        let request = OfferRequest { runner, held, degraded, now };
        match self.claim_with(drafts, request, accepted, |lease, _| {
            serde_json::to_string(&LeaseResponse { lease: Some(lease), retry_after_ms: None })
                .map_err(|_shape| rejected(DETAIL_EVENT_MALFORMED))
        }).await? {
            Some(body) => body,
            None => no_work(runner, "no eligible accepted work"),
        }
    }

    /// Serialize each domain's offer inside the values that own its borrowed fields.
    /// The returned metadata carries its target without another database lookup.
    ///
    /// # Errors
    /// Preserves host selection, policy, billing and durable placement failures.
    pub async fn claim_with<T, F>(
        &self,
        drafts: &afd_authoring::Drafts,
        request: OfferRequest<'_>,
        accepted: AcceptedKinds,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(TaggedLease<'a, afd_authoring::types::Candidate>, Offered) -> T + Send,
    {
        if request.degraded { return Ok(None); }
        let hosts =
            afd_runner::Runners::new(self.leases.pool().clone(), self.leases.entropy().clone());
        let Some(ticket) = hosts.claim_offer(request.runner, accepted).await? else {
            return Ok(None);
        };
        for kind in ticket.order() {
            match kind {
                LeaseKind::Authoring => {
                    if let Some(lease) = drafts.claim_offered(request.runner, ticket).await? {
                        let metadata = Offered {
                            lease_id: Uuid7::parse(&lease.lease_id)?,
                            expires_at: lease.expires_at,
                            target: None,
                        };
                        return Ok(Some(emit(TaggedLease::Authoring(Box::new(lease)), metadata)));
                    }
                }
                LeaseKind::Fleet => {
                    if let Polled::Lease(body) =
                        self.poll_with(request, Some(ticket), |lease, metadata| {
                            emit(TaggedLease::Fleet(Box::new(lease)), metadata)
                        }).await?
                    {
                        return Ok(Some(body));
                    }
                }
            }
        }
        Ok(None)
    }
}
