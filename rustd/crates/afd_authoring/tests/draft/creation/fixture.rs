//! Each setup owns its Postgres rows and Dragonfly stream, including on retry.

use super::super::{
    fixture::{Fixture, now},
    mint,
};
use afd_admission::Admissions;
use afd_authoring::{
    Creations, SavedFleet, Setup,
    types::{DraftPatch, SaveFleet, SaveSource, Start},
};
use afd_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_dragonfly::{
    Dragonfly, FleetStreams, ReadyIndex,
    config::{DragonflyConfig, DragonflyRole},
};
use serde_json::json;
use std::sync::Arc;

/// What [`Case::admissions`] reads for a fleet that admitted nothing.
pub(super) const NO_ADMISSIONS: [(String, Option<String>); 0] = [];

/// What [`Case::stream_events`] reads for a fleet nothing was appended to.
pub(super) const NO_EVENTS: [String; 0] = [];

/// The state `saved` reports for its setup check of `kind`.
pub(super) fn check<'a>(saved: &'a SavedFleet, kind: &str) -> &'a str {
    saved
        .setup
        .checks
        .iter()
        .find(|check| check.kind == kind)
        .map(|check| check.state)
        .expect("setup reports every check a proof names")
}

pub(super) struct Case {
    pub(super) base: Fixture,
    pub(super) setup: Setup,
    pub(super) creations: Creations,
    pub(super) queue: Dragonfly,
}

impl Case {
    pub(super) async fn create() -> Self {
        Self::over(Fixture::create().await).await
    }

    /// A case with a database of its own, for a proof whose fault would reach
    /// every other proof's rows.
    pub(super) async fn isolated() -> Self {
        Self::over(Fixture::isolated().await).await
    }

    async fn over(base: Fixture) -> Self {
        let config = DragonflyConfig::from_url(
            DragonflyRole::Default,
            ProcessEnv
                .get("TEST_DRAGONFLY_URL")
                .expect("live queue URL"),
        );
        let queue = afd_dragonfly::test_util::connect_live(&config)
            .await
            .expect("live cluster");
        let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
            .expect("fixture encryption key");
        let setup = Setup::new(base.drafts.clone(), Arc::new(key));
        let admissions = Admissions::new(base.db.clone(), queue.clone(), Entropy::new());
        let creations = Creations::new(setup.clone(), queue.clone(), admissions);
        Self {
            base,
            setup,
            creations,
            queue,
        }
    }

    pub(super) async fn request(&self, name: &str, start: Start) -> (Uuid7, SaveFleet) {
        let draft = self
            .base
            .drafts
            .create(self.base.access(), self.base.request(mint(), None), now())
            .await
            .expect("idle private setup")
            .draft;
        let patch: DraftPatch = serde_json::from_value(json!({
            "runtime_model_ref":self.base.model, "instance_name":name
        }))
        .expect("runtime and instance choices");
        self.base
            .drafts
            .patch(
                self.base.access(),
                &draft.id,
                Some(&draft.etag),
                patch,
                now(),
            )
            .await
            .expect("select reviewed choices");
        let reviewed = self
            .setup
            .read_draft(self.base.access(), &draft.id)
            .await
            .expect("review");
        (
            draft.id,
            SaveFleet {
                source: SaveSource::Draft {
                    draft_candidate_id: reviewed.candidate.expect("candidate").id,
                },
                name: name.to_owned(),
                review_digest: reviewed.review.expect("live review").review_digest,
                start,
            },
        )
    }

    pub(super) async fn fund(&self) {
        sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
            .bind(self.base.tenant.as_str()).bind(crate::draft::FUNDED_NANOS)
            .execute(&mut *self.base.connection().await).await.expect("wallet covers activation");
    }

    pub(super) async fn save(&self, key: &Uuid7, request: SaveFleet) -> SavedFleet {
        self.creations
            .save(self.base.access(), key, request.clone(), now())
            .await
            .expect("retained Save")
    }

    pub(super) async fn admissions(&self, fleet: &Uuid7) -> Vec<(String, Option<String>)> {
        sqlx::query_as("SELECT id::text,receipt FROM fleet.admissions WHERE fleet_id=$1::uuid ORDER BY sequence")
            .bind(fleet.as_str()).fetch_all(&mut *self.base.connection().await).await.expect("durable admissions")
    }

    /// The logical event ID every physical entry on `fleet`'s stream carries.
    /// A replayed append may land twice under one ID; a second run never does.
    pub(super) async fn stream_events(&self, fleet: &Uuid7) -> Vec<String> {
        afd_dragonfly::test_util::fleet_entries(&self.queue, fleet.as_str())
            .await
            .expect("read the fleet stream")
            .iter()
            .map(|entry| {
                entry
                    .field(afd_wire::event::field::EVENT_ID)
                    .expect("every entry names its event")
                    .to_owned()
            })
            .collect()
    }

    pub(super) async fn fleet_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1::uuid")
            .bind(self.base.workspace.as_str())
            .fetch_one(&mut *self.base.connection().await)
            .await
            .expect("only owned fleets")
    }

    pub(super) async fn cleanup(self) {
        let fleets: Vec<String> =
            sqlx::query_scalar("SELECT id::text FROM fleet.fleets WHERE workspace_id=$1::uuid")
                .bind(self.base.workspace.as_str())
                .fetch_all(&mut *self.base.connection().await)
                .await
                .expect("owned streams");
        for fleet in fleets {
            FleetStreams::new(self.queue.clone())
                .forget(&fleet)
                .await
                .expect("remove owned stream");
            ReadyIndex::new(self.queue.clone())
                .force_clear(&fleet)
                .await
                .expect("remove owned hint");
        }
        self.base.cleanup().await;
    }
}
