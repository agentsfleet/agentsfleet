//! A run's decision profile cannot change under a decision it already owes:
//! an unsent call is never sent under another profile, and an unknown
//! revision serves nothing.

use af_core::error_code;
use af_wire::task::DecisionAssessment;
use afd_fleet::task::TaskLifecycle as _;

use super::{
    super::fixture::{Case, exercise},
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

async fn unavailable(fixture: &Decisions<'_>) {
    let failure = fixture
        .tasks
        .reserve_decision(
            fixture.case.run(),
            &fixture.reservation(DecisionAssessment::Message, None),
        )
        .await
        .expect_err("no decision is sent");
    assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
}

/// Leaves one message call reserved and unsent: the payer cannot fund it.
async fn unsent(fixture: &Decisions<'_>) -> i64 {
    let case = fixture.case;
    let balance = case
        .held
        .fixtures
        .balance(&case.held.tenant)
        .await
        .expect("a funded payer");
    case.held.fixtures.seed_wallet(&case.held.tenant, 0).await;
    unavailable(fixture).await;
    assert_eq!(fixture.counts().await, (1, 1, 0), "one unsent call is kept");
    balance
}

// Value: protects=an unsent call reserved under one profile is never sent under
// another; fails_when=the profile-switch barrier or the issue-time profile check
// is dropped, and the retry sends the old call under the new profile; why_new=
// both refusals had no case; seam=the production reserve verb over live Postgres,
// the run's assignment rewritten between a refused and a funded retry;
// no_claim=resolving the retained call, which the funding suite covers.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unsent_decision_is_not_sent_under_a_changed_profile() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let balance = unsent(&fixture).await;
        case.held
            .fixtures
            .seed_wallet(&case.held.tenant, balance)
            .await;
        reassign(case, REVISION, &afd_db::test_util::mint_id().to_string()).await;
        unavailable(&fixture).await;
        assert_eq!(
            fixture.counts().await,
            (1, 1, 0),
            "nothing more is reserved"
        );
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let balance = unsent(&fixture).await;
        case.held
            .fixtures
            .seed_wallet(&case.held.tenant, balance)
            .await;
        // The same revision, with a profile the call was not reserved under.
        reassign(case, INPUT_BYTES, "1").await;
        unavailable(&fixture).await;
        assert_eq!(fixture.counts().await, (1, 1, 0), "the call stays unsent");
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
