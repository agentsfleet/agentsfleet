//! The production verbs a saturation host calls, behind [`Host`].
//!
//! The daemon's `Plane` and `Drafts`, composed the way its root composes them
//! and sharing one biller, so a setup chat claim and the fleet poll it competes
//! with move money through the same path they do in production.

use std::sync::Arc;

use afd_authoring::Drafts;
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_fleet::lease::Plane;
use afd_wire::authoring::{Knowledge, Meter, Renew};
use afd_wire::lease::{AcceptedKinds, LeaseResponse, TaggedLease};

use super::requester::{Held, Host, Polled};
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
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
    plane: Plane,
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
        let answer = self
            .plane
            .claim(
                &self.drafts,
                runner,
                &[],
                AcceptedKinds::ALL,
                false,
                clock::now(),
            )
            .await;
        answer.map_or(Ok(Polled::Refused), |body| polled(&body))
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

/// What a lease answer's body says was issued.
fn polled(body: &str) -> Result<Polled> {
    let response: LeaseResponse<'_> =
        serde_json::from_str(body).map_err(|source| ErrorKind::LeaseUnreadable { source })?;
    Ok(match response.lease {
        None => Polled::Nothing,
        Some(TaggedLease::Fleet(_)) => Polled::Fleet,
        Some(TaggedLease::Authoring(lease)) => Polled::Chat(Held {
            lease: Uuid7::parse(&lease.lease_id)?,
            fence: lease.fence.clone(),
        }),
    })
}
