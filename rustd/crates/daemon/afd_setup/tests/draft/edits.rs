//! A model selection changes future execution while preserving reviewed content.

use super::{
    fixture::{Fixture, now},
    mint,
};
use af_core::error_code::{INVALID_REQUEST, SETUP_PRECONDITION, SETUP_STALE};
use afd_setup::types::DraftPatch;
use serde_json::json;

/// Appends a copy of draft `$1`'s head at the last revision a candidate can
/// hold, id `$2`: a proposal changes only when installed, so the head cannot be renumbered.
const LAST_REVISION: &str = "INSERT INTO fleet.revisions \
     (id,draft_id,revision,definition,canonical_digest,policy_semantics,installed,created_at,updated_at) \
     SELECT $2,d.id,9223372036854775807,c.definition,c.canonical_digest,\
     c.policy_semantics,false,now(),now() \
     FROM fleet.revisions c JOIN setup.drafts d ON d.head_revision_id=c.id \
     WHERE d.id=$1";
/// Moves draft `$1`'s head to candidate `$2`.
const MOVE_HEAD: &str = "UPDATE setup.drafts SET head_revision_id=$2 WHERE id=$1";

fn patch(value: serde_json::Value) -> DraftPatch {
    serde_json::from_value(value).expect("typed patch")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn setup_model_replacement_preserves_candidate_and_obeys_strong_tag() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("native candidate")
        .draft;
    let candidate = created.candidate.as_ref().expect("initial candidate");
    let replacement = || patch(json!({"setup_model_ref": f.other_model}));
    let error = f
        .drafts
        .patch(f.access(), &created.id, None, replacement(), now())
        .await
        .expect_err("missing tag");
    assert_eq!(error.code(), SETUP_PRECONDITION);
    let changed = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&created.etag),
            replacement(),
            now(),
        )
        .await
        .expect("explicit replacement");
    assert_eq!(changed.setup_model_ref, f.other_model.to_string());
    assert_ne!(changed.etag, created.etag);
    assert_eq!(
        changed.candidate.as_ref().expect("candidate retained").id,
        candidate.id
    );
    assert_eq!(
        changed
            .candidate
            .as_ref()
            .expect("digest retained")
            .canonical_digest,
        candidate.canonical_digest
    );
    assert_stale(&f, &created, &changed).await;
    assert_eq!(f.counts().await, (1, 1, 0));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn invalid_full_edit_rolls_back_title_and_candidate_pointer() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("native candidate")
        .draft;
    // A zero ceiling is refused only once the whole candidate is revalidated,
    // after the title has been written inside the same transaction.
    let invalid = patch(json!({"title":"must roll back", "config_ops":[
        {"op":"replace","path":"/config/x-agentsfleet/budget/daily_nanos","value":"0"}
    ]}));
    let error = f
        .drafts
        .patch(f.access(), &created.id, Some(&created.etag), invalid, now())
        .await
        .expect_err("invalid full candidate must be refused");
    assert_eq!(error.code(), INVALID_REQUEST);
    let unchanged = f
        .drafts
        .read(f.access(), &created.id)
        .await
        .expect("reload after refusal");
    assert_eq!(unchanged.title, created.title);
    assert_eq!(unchanged.etag, created.etag);
    assert_eq!(f.counts().await, (1, 1, 0));
    f.cleanup().await;
}

async fn assert_stale(
    f: &Fixture,
    created: &afd_setup::types::Draft,
    changed: &afd_setup::types::Draft,
) {
    let error = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&created.etag),
            patch(json!({"title":"stale"})),
            now(),
        )
        .await
        .expect_err("old tag cannot overwrite replacement");
    assert_eq!(error.code(), SETUP_STALE);
    assert_eq!(error.etag(), Some(changed.etag.as_str()));
}

// Value: protects=an edit needs a candidate to edit, and a head whose revision
// cannot advance refuses the edit rather than overwriting it;
// fails_when=either refusal is dropped and the edit writes; seam=Drafts::patch
// over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_edit_with_no_candidate_or_no_next_revision_is_refused() {
    let f = Fixture::create().await;
    let mut request = f.request(mint(), None);
    request.source = afd_setup::types::Source::Blank;
    let blank = f
        .drafts
        .create(f.access(), request, now())
        .await
        .expect("a blank chat")
        .draft;
    let model = || patch(json!({"runtime_model_ref": f.model}));
    let refused = f
        .drafts
        .patch(f.access(), &blank.id, Some(&blank.etag), model(), now())
        .await
        .expect_err("nothing to edit");
    assert_eq!(refused.code(), INVALID_REQUEST);
    assert_eq!(
        refused.detail(),
        "initialize a candidate before editing its setup"
    );

    let headed = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("a chat with setup")
        .draft;
    let last = mint();
    let mut connection = f.connection().await;
    for statement in [LAST_REVISION, MOVE_HEAD] {
        let written = sqlx::query(statement)
            .bind(&headed.id)
            .bind(&last)
            .execute(&mut *connection)
            .await
            .expect("a head at the last revision");
        assert_eq!(written.rows_affected(), 1);
    }
    drop(connection);
    let refused = f
        .drafts
        .patch(f.access(), &headed.id, Some(&headed.etag), model(), now())
        .await
        .expect_err("no next revision");
    assert_eq!(refused.code(), INVALID_REQUEST);
    assert_eq!(refused.detail(), "candidate revision is exhausted");
    let candidates: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.revisions WHERE draft_id=$1")
            .bind(&headed.id)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("candidate census");
    assert_eq!(candidates, 2, "the refused edit wrote no candidate");
    f.cleanup().await;
}
