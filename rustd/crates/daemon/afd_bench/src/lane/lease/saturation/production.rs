//! The production verbs a saturation host calls, behind [`Host`].
//!
//! The daemon's `LeaseService` and `Drafts`, composed the way its root composes them
//! and sharing one biller, so a setup chat claim and the fleet poll it competes
//! with move money through the same path they do in production.

use std::sync::Arc;

use af_core::clock;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use af_wire::authoring::{Knowledge, Meter, Renew};
use af_wire::lease::{AcceptedKinds, TaggedLease};
use afd_authoring::Drafts;
use afd_fleet::lease::{LeaseService, OfferRequest};

use super::requester::{Held, Host, Polled};
use crate::datastores::Datastores;
use crate::error::Result;
use crate::lane::lease::drain::stage;

/// What a host that has made no model call yet observes on renewal.
const IDLE: Meter = Meter {
    knowledge: Knowledge::Known,
    calls: 0,
    input_tokens: None,
    output_tokens: None,
    runtime_ms: 0,
    provider_cost_nanos: None,
};

/// The runner route's two verbs over the lane's datastores.
#[derive(Debug, Clone)]
pub(super) struct Production {
    plane: LeaseService,
    pub(super) drafts: Drafts,
}

impl Production {
    /// The plane and the drafts over `stores`, resolving credentials sealed
    /// under `kek`.
    pub(super) fn new(stores: &Datastores, kek: &Arc<Kek>) -> Self {
        let plane = stage::plane(stores, kek);
        let drafts = Drafts::new(
            stores.database.clone(),
            Entropy::new(),
            Arc::clone(&plane.biller),
        );
        Self { plane, drafts }
    }
}

impl Host for Production {
    async fn poll(&self, runner: &Uuid7) -> Result<Polled> {
        let offered = self
            .plane
            .claim_with(
                &self.drafts,
                OfferRequest {
                    runner,
                    held: &[],
                    degraded: false,
                    now: clock::now(),
                },
                AcceptedKinds::ALL,
                |lease, _metadata| match lease {
                    TaggedLease::Fleet(_) => Ok(Polled::Fleet),
                    TaggedLease::Authoring(lease) => Ok(Polled::Chat(Held {
                        lease: Uuid7::parse(&lease.lease_id)?,
                        fence: lease.fence.clone(),
                    })),
                },
            )
            .await;
        match offered {
            Ok(Some(issued)) => issued,
            Ok(None) => Ok(Polled::Nothing),
            Err(_) => Ok(Polled::Refused),
        }
    }

    async fn renew(&self, runner: &Uuid7, held: &Held) -> bool {
        let request = Renew {
            fence: held.fence.clone(),
            usage: IDLE,
        };
        self.drafts
            .renew(runner, &held.lease, request)
            .await
            .is_ok()
    }
}
