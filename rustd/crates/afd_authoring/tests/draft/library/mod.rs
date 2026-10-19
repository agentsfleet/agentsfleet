//! Explicit publication and pinned history over the real native schema.

mod authority;
mod funnel;
mod listing;
mod publication;
mod revisions;
mod staging;

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{
    Setup,
    types::{Draft, DraftPatch, PublishLibrary, Published},
};
use afd_crypto::secret::Kek;
use std::sync::Arc;

struct Case {
    base: Fixture,
    setup: Setup,
}

impl Case {
    async fn create() -> Self {
        let base = Fixture::create().await;
        let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
            .expect("fixture key");
        let setup = Setup::new(base.drafts.clone(), Arc::new(key));
        Self { base, setup }
    }

    async fn draft(&self) -> Draft {
        self.base
            .drafts
            .create(self.base.access(), self.base.request(mint(), None), now())
            .await
            .expect("idle draft")
            .draft
    }

    async fn publish(&mut self, draft: &Draft) -> Published {
        let published = self
            .setup
            .publish(
                self.base.access(),
                &mint(),
                PublishLibrary {
                    draft_candidate_id: draft.candidate.as_ref().expect("candidate").id.clone(),
                    label: "ARGO".into(),
                },
                now(),
            )
            .await
            .expect("explicit publication");
        self.base.own_library(&published.library_id);
        published
    }

    async fn edit(&self, draft: &Draft) -> Draft {
        let current = self
            .base
            .drafts
            .read(self.base.access(), &draft.id)
            .await
            .expect("current tag");
        let patch: DraftPatch = serde_json::from_value(serde_json::json!({"instruction_edits":[
            {"start_byte":0,"end_byte":0,"text":"Continue checking.\n"}]}))
        .expect("edit input");
        self.base
            .drafts
            .patch(
                self.base.access(),
                &draft.id,
                Some(&current.etag),
                patch,
                now(),
            )
            .await
            .expect("continue draft")
    }

    async fn counts(&self) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM authoring.library_entries WHERE tenant_id=$1::uuid),
        (SELECT count(*) FROM authoring.library_revisions r JOIN authoring.library_entries l ON l.id=r.library_entry_id WHERE l.tenant_id=$1::uuid),
        (SELECT count(*) FROM fleet.fleets WHERE tenant_id=$1::uuid)")
            .bind(self.base.tenant.as_str()).fetch_one(&mut *self.base.connection().await).await.expect("owned counts")
    }
}
