//! One native claim selects between independently authorized placement domains.

use af_core::id::Uuid7;
use af_observability::verb_span::{OfferStage, VerbClock};
use af_wire::lease::{AcceptedKinds, LeaseKind, TaggedLease};

use super::LeaseService;
use super::{OfferRequest, Offered};
use crate::Result;

impl LeaseService {
    /// Serialize each domain's offer inside the values that own its borrowed fields.
    /// The returned metadata carries its target without another database lookup.
    /// Marks each offer stage it reaches on `clock`; a pass that selects
    /// nothing marks none.
    ///
    /// # Errors
    /// Preserves host selection, policy, billing and durable placement failures.
    ///
    /// `draft_leases` is whether the runner's protocol revision takes a
    /// draft-owned lease: such a runner gets setup work on the common lease
    /// path ([`crate::lease::draft`]), any other the authoring attempt path.
    pub async fn claim_with<T, F>(
        &self,
        drafts: &afd_setup::Drafts,
        mut request: OfferRequest<'_>,
        accepted: AcceptedKinds,
        draft_leases: bool,
        clock: &mut VerbClock<OfferStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(TaggedLease<'a, afd_setup::types::Candidate>, Offered) -> T + Send,
    {
        if request.degraded {
            return Ok(None);
        }
        let hosts =
            afd_runner::Runners::new(self.leases.pool().clone(), self.leases.entropy().clone());
        let Some(ticket) = hosts.claim_offer(request.runner, accepted).await? else {
            return Ok(None);
        };
        for kind in ticket.order() {
            match kind {
                LeaseKind::Authoring if draft_leases => {
                    clock.restart_if_staged();
                    if let Some(offer) = self
                        .claim_draft(drafts, request.runner, ticket, request.now)
                        .await?
                    {
                        clock.mark(OfferStage::Claim);
                        let metadata = offer.offered();
                        let encoded = emit(TaggedLease::Fleet(Box::new(offer.payload())), metadata);
                        clock.mark(OfferStage::Encode);
                        return Ok(Some(encoded));
                    }
                }
                LeaseKind::Authoring => {
                    // A fleet claim the gates refused earlier in this pass
                    // keeps its own span; this claim starts a fresh one, so
                    // its own database time is its `claim` stage.
                    clock.restart_if_staged();
                    if let Some(lease) = drafts.claim_offered(request.runner, ticket).await? {
                        clock.mark(OfferStage::Claim);
                        let metadata = Offered {
                            lease_id: Uuid7::parse(&lease.lease_id)?,
                            expires_at: lease.expires_at,
                            target: None,
                            reported: None,
                            draft: None,
                        };
                        let encoded = emit(TaggedLease::Authoring(Box::new(lease)), metadata);
                        clock.mark(OfferStage::Encode);
                        return Ok(Some(encoded));
                    }
                }
                LeaseKind::Fleet => {
                    if let Some(body) = self
                        .claim_fleet_with(&mut request, Some(ticket), clock, |lease, metadata| {
                            emit(TaggedLease::Fleet(Box::new(lease)), metadata)
                        })
                        .await?
                    {
                        return Ok(Some(body));
                    }
                }
            }
        }
        Ok(None)
    }
}
