//! A run's decision profile cannot change under a decision it already owes: a call
//! issued and not yet receipted blocks its subject under another revision, a changed
//! profile of the same revision replays that call, and an unknown revision serves nothing.

use af_core::error_code;
use af_wire::durable_task::DecisionAssessment;
use afd_fleet::durable_task::DurableTaskLifecycle as _;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
};

/// Rewrites the run's frozen decision assignment with `statement`, which binds
/// the run as `$1` and `value` as `$2`.
async fn reassign(case: &Case, statement: &'static str, value: &str) {
    sqlx::query(statement)
        .bind(&case.held.run_key)
        .bind(value)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the run's assignment changes");
}

/// Names another revision in the assignment.
const REVISION: &str = "UPDATE thread.runs SET decision_snapshot = \
    jsonb_set(decision_snapshot, '{revision_id}', to_jsonb($2::text)) WHERE id=$1";
/// Keeps the revision and narrows the profile's input bound by `$2` bytes.
const INPUT_BYTES: &str = "UPDATE thread.runs SET decision_snapshot = \
    jsonb_set(decision_snapshot, '{profile,input_max_bytes}', \
    to_jsonb((decision_snapshot #>> '{profile,input_max_bytes}')::bigint - ($2::text)::bigint)) \
    WHERE id=$1";
/// A second reviewed revision of the model `$2`'s revision belongs to, enabled as it is.
const SECOND_REVISION: &str = "INSERT INTO model.decision_revisions \
    (id, platform_model_id, revision, definition, created_at) \
    SELECT $1, platform_model_id, revision || '-next', definition, now() \
    FROM model.decision_revisions WHERE id = $2";

async fn unavailable(fixture: &Decisions<'_>) {
    let failure = fixture
        .tasks
        .reserve_decision(
            fixture.case.run(),
            &fixture.reservation(DecisionAssessment::Message, None),
        )
        .await
        .expect_err("no decision is sent");
    assert_eq!(
        failure.code(),
        error_code::DURABLE_TASK_DECISION_UNAVAILABLE
    );
}

/// Issues one message call and leaves it without a receipt.
async fn unreceipted(fixture: &Decisions<'_>) {
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (issued, _body) = fixture.reserve(&input).await;
    assert!(issued);
    assert_eq!(fixture.counts().await, (1, 1, 0), "one call is issued");
}

// Value: protects=a subject with an unreceipted call takes no call under another
// revision; fails_when=the profile-switch barrier is dropped, and the subject is
// assessed again under the new revision while the first call's usage is unknown;
// why_new=the barrier's only case relied on a retained unissued call, which no longer
// exists; seam=the production reserve verb over live Postgres, the run's assignment
// moved to a second enabled revision of the same model; no_claim=receipting the call.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unreceipted_call_blocks_its_subject_under_a_new_profile() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        unreceipted(&fixture).await;
        let next = afd_db::test_util::mint_id();
        sqlx::query(SECOND_REVISION)
            .bind(&next)
            .bind(id(&fixture.assignment.revision_id))
            .execute(&mut *case.held.fixtures.database.acquire().await.expect("writer"))
            .await
            .expect("a second enabled revision");
        reassign(case, REVISION, next.as_str()).await;
        unavailable(&fixture).await;
        assert_eq!(fixture.counts().await, (1, 1, 0), "nothing more is issued");
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a call issued under one profile is not issued again when the same
// revision's profile changes; fails_when=the assessment lookup stops finding the saved
// call, and a second key leaves for one assessment; why_new=the case rode on a retained
// unissued call; seam=the production reserve verb, the run's profile narrowed by one
// byte under the same revision; no_claim=which profile a receipt is judged under.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_changed_profile_of_one_revision_replays_the_issued_call() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        unreceipted(&fixture).await;
        reassign(case, INPUT_BYTES, "1").await;
        let again = fixture.reservation(DecisionAssessment::Message, None);
        let (issued, body) = fixture.reserve(&again).await;
        assert!(!issued, "the saved call answers: {body}");
        assert_eq!(body.get("state"), Some(&serde_json::json!("issued")));
        assert_eq!(fixture.counts().await, (1, 1, 0));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a run whose assignment names a revision the catalogue never
// enabled reserves nothing; fails_when=the activation lookup's absence stops
// refusing; why_new=the refusal had no case; seam=the production reserve verb
// over live Postgres, the run's revision replaced; no_claim=catalogue changes.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_revision_the_catalogue_never_enabled_serves_no_decision() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        reassign(case, REVISION, &afd_db::test_util::mint_id().to_string()).await;
        unavailable(&fixture).await;
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
