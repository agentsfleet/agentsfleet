//! Live observations cannot silently change the setup a person reviewed.

mod authority;
mod identity;

use afd_authoring::{
    Setup,
    types::{DraftPatch, Review},
};
use afd_crypto::secret::Kek;
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

async fn draft(f: &Fixture, joined: bool) -> afd_authoring::types::Draft {
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

async fn review(f: &Fixture, draft: &afd_authoring::types::Draft, joined: bool) -> Review {
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
