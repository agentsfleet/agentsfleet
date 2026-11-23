//! Live observations cannot silently change the setup a person reviewed.

mod authority;
mod identity;

use af_crypto::secret::Kek;
use afd_setup::{
    Setup,
    types::{DraftPatch, Review},
};
use serde_json::json;
use std::sync::Arc;

use super::{
    fixture::{Fixture, now},
    mint,
};

fn setup(f: &Fixture) -> Setup {
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    Setup::new(f.drafts.clone(), Arc::new(key))
}

async fn draft(f: &Fixture, joined: bool) -> afd_setup::types::Draft {
    let created = f
        .drafts
        .create(
            if joined {
                f.member_access()
            } else {
                f.access()
            },
            f.request(mint(), None),
            now(),
        )
        .await
        .expect("create private native setup")
        .draft;
    let patch: DraftPatch = serde_json::from_value(json!({"runtime_model_ref":f.model}))
        .expect("select owner's runtime reference");
    f.drafts
        .patch(
            if joined {
                f.member_access()
            } else {
                f.access()
            },
            &created.id,
            Some(&created.etag),
            patch,
            now(),
        )
        .await
        .expect("idle runtime selection")
}

async fn review(f: &Fixture, draft: &afd_setup::types::Draft, joined: bool) -> Review {
    setup(f)
        .read_draft(
            if joined {
                f.member_access()
            } else {
                f.access()
            },
            &draft.id,
        )
        .await
        .expect("current private review")
        .review
        .expect("candidate has review")
}

// Value: protects=a setup with no instructions reviews as blocked on its
// definition, so Save is not offered for it; fails_when=the definition check
// reads Ready whatever the setup holds; seam=Setup::read_draft over live
// Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_setup_with_no_instructions_reviews_as_blocked_on_its_definition() {
    let f = Fixture::create().await;
    let mut value = serde_json::to_value(super::definition()).expect("the fixture setup");
    if let Some(text) = value.get_mut("instructions_md") {
        *text = json!("   \n");
    }
    let mut request = f.request(mint(), None);
    request.source = afd_setup::types::Source::Definition {
        definition: Box::new(serde_json::from_value(value).expect("an unsaveable setup parses")),
    };
    let created = f
        .drafts
        .create(f.access(), request, now())
        .await
        .expect("a chat on an unsaveable setup")
        .draft;
    let definition = |review: &Review| {
        review
            .readiness
            .iter()
            .find(|check| check.kind == afd_setup::types::CheckKind::Definition)
            .map(|check| check.state)
    };
    let blocked = setup(&f)
        .read_draft(f.access(), &created.id)
        .await
        .expect("the review reads")
        .review
        .expect("candidate has review");
    assert_eq!(
        definition(&blocked),
        Some(afd_setup::types::CheckState::Blocked)
    );
    let saveable = draft(&f, false).await;
    assert_eq!(
        definition(&review(&f, &saveable, false).await),
        Some(afd_setup::types::CheckState::Ready)
    );
    f.cleanup().await;
}
