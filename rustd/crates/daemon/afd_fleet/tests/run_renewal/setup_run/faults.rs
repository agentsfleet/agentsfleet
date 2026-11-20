//! What a draft that cannot run does to a poll: it fails its own turn, or
//! waits, and never stalls the walk, the fleet work after it, or a runner it
//! does not belong to.

use af_core::{error_code, test_util::trace::Capture};
use af_observability::verb_span::VerbClock;
use af_wire::run::{AcceptedKinds, TaggedRun};
use afd_fleet::run::AssignRequest;
use afd_setup::setup_run::EVENT_TURN_FAILED;

use super::Case;
use super::placement::tenant_runner;

impl Case {
    /// The host's poll with the fixture fleet held, so its fleet arm may take
    /// the fleet's lapsed turn: `Some(true)` for a setup run, `Some(false)`
    /// for a fleet run, `None` for no run.
    async fn poll_holding_the_fleet(&self) -> Result<Option<bool>, afd_fleet::Error> {
        self.running
            .fixtures
            .plane()
            .assign_with(
                &self.drafts,
                AssignRequest {
                    runner: &self.running.runner,
                    held: std::slice::from_ref(&self.running.fleet),
                    degraded: false,
                    now: self.running.now,
                    recovery: None,
                },
                AcceptedKinds::ALL,
                &mut VerbClock::untimed(),
                |run, _metadata| {
                    let TaggedRun::Fleet(run) = run;
                    run.owner.setup().is_some()
                },
            )
            .await
    }

    /// The fixture fleet's thread fence.
    async fn fleet_fence(&self) -> i64 {
        sqlx::query_scalar("SELECT fencing_token FROM thread.threads WHERE fleet_id = $1")
            .bind(&self.running.fleet)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the fleet's thread")
    }
}

// Value: protects=one draft's broken key never stalls assignment for the runner;
// fails_when=a stored-key fault propagates out of the setup arm again, rolling the
// pass back before the fleet arm; why_new=a malformed key failed every poll;
// seam=the production poll over live Postgres, with the seat's key resealed empty.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_draft_whose_key_will_not_serve_fails_its_turn_and_the_fleet_is_still_assigned() {
    let case = Case::queued().await;
    case.seat.break_key(&case.running.fixtures.database).await;
    let before = case.fleet_fence().await;

    let captured = Capture::install();
    let answer = case.poll_holding_the_fleet().await;
    let failed = captured.only(EVENT_TURN_FAILED);
    drop(captured);

    let answer = answer.expect("a draft's broken key does not fail the poll");
    assert_ne!(answer, Some(true), "no setup run was issued");
    assert!(
        case.fleet_fence().await > before,
        "the fleet arm ran and took the fleet's lapsed turn"
    );
    let turn = case.turn_on_head().await;
    assert_eq!(turn.status, "failed", "{turn:?}");
    assert_eq!(turn.failure_reason.as_deref(), Some("model_unavailable"));
    assert_eq!(case.runs().await, Vec::new(), "the draft holds no run");
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::SETUP_MODEL.as_str())
    );
    assert_eq!(
        failed.field("cause_error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(failed.field("draft_id"), Some(case.draft.id.as_str()));
    assert_eq!(failed.field("reason"), Some("model_unavailable"));
    case.cleanup().await;
}

// Value: protects=a draft's setup run, and its tenant's key, never reach another
// tenant's runner; fails_when=both the scan's and the assignment's tenant checks are
// dropped; why_new=replaces the authoring-era foreign-runner case; seam=the production
// poll over live Postgres, from a runner another tenant registered.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_runner_of_another_tenant_is_never_given_a_drafts_setup_run() {
    let case = Case::queued().await;
    let stranger = case.running.fixtures.seed_tenant().await;
    let runner = tenant_runner(&case.running.fixtures, &stranger).await;

    for poll in 1..=2 {
        assert!(
            case.take(&runner).await.is_none(),
            "poll {poll}: another tenant's runner takes no draft run"
        );
    }
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    let queued = case.turn_on_head().await;
    assert_eq!(queued.status, "queued", "the turn waits: {queued:?}");
    assert_eq!(queued.failure_reason, None, "and is not failed: {queued:?}");

    let taken = case.take_on_host().await;
    assert_eq!(taken.fence, 1, "the tenant's own host still takes it");
    case.cleanup().await;
}

// Value: protects=a payer who cannot cover one run is told so and is not run;
// fails_when=the balance gate is dropped from the assignment, or its refusal stalls
// the pass; why_new=the balance arm had no live case; seam=the production poll over
// live Postgres, with the seat's wallet emptied.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_payer_who_cannot_cover_a_run_fails_the_turn_as_insufficient_balance() {
    let case = Case::queued().await;
    let emptied = sqlx::query("UPDATE billing.wallets SET balance_nanos = 0 WHERE tenant_id = $1")
        .bind(&case.seat.tenant)
        .execute(&mut *case.connection().await)
        .await
        .expect("the wallet empties");
    assert_eq!(emptied.rows_affected(), 1);

    let captured = Capture::install();
    let taken = case.take(&case.running.runner).await;
    let failed = captured.only(EVENT_TURN_FAILED);
    drop(captured);

    assert!(taken.is_none(), "an unfunded turn is not run");
    let turn = case.turn_on_head().await;
    assert_eq!(turn.status, "failed", "{turn:?}");
    assert_eq!(turn.failure_reason.as_deref(), Some("insufficient_balance"));
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::SETUP_BALANCE.as_str())
    );
    assert_eq!(failed.field("cause_error_code"), None);
    case.cleanup().await;
}

// Value: protects=a creator who lost the workspace has their queued turn cancelled,
// never run on their key; fails_when=the revoked branch stops cancelling, or assigns
// the turn; why_new=the revoked-creator branch had no live case; seam=the production
// poll over live Postgres, the creator moved to another tenant with no membership.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_revoked_creators_queued_turn_is_cancelled_and_not_run() {
    let case = Case::queued().await;
    let elsewhere = case.running.fixtures.seed_tenant().await;
    let moved = sqlx::query("UPDATE identity.users SET tenant_id = $2 WHERE id = $1")
        .bind(&case.seat.owner)
        .bind(&elsewhere)
        .execute(&mut *case.connection().await)
        .await
        .expect("the creator leaves the seat's tenant");
    assert_eq!(moved.rows_affected(), 1);

    assert!(
        case.take(&case.running.runner).await.is_none(),
        "a revoked creator's turn is not run"
    );
    let turn = case.turn_on_head().await;
    assert_eq!(turn.status, "cancelled", "{turn:?}");
    assert_eq!(turn.failure_reason, None, "{turn:?}");
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    assert!(
        case.take(&case.running.runner).await.is_none(),
        "a cancelled turn is never taken again"
    );
    case.cleanup().await;
}

// Value: protects=a turn whose run went with its deleted runner is run again, not
// stranded; fails_when=the scan or the head read requires a run row for a running
// head again; why_new=a deleted runner's turn waited forever; seam=the production
// poll over live Postgres, the run row removed as the runner's cascade removes it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_running_turn_whose_run_went_with_its_runner_is_assigned_again() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    case.lapse(&first.run_id()).await;
    // `thread.runs.runner_id` cascades: a deleted runner leaves its running
    // turn with no run row at all, which is what this delete leaves.
    let gone = sqlx::query("DELETE FROM thread.runs WHERE id = $1")
        .bind(first.run_id())
        .execute(&mut *case.connection().await)
        .await
        .expect("the run goes");
    assert_eq!(gone.rows_affected(), 1);
    assert_eq!(case.turn(&first.turn_id()).await.status, "running");

    let again = case.take_on_host().await;
    assert_eq!(again.turn, first.turn, "the same turn, run again");
    assert_eq!(again.fence, 2, "at the thread's next fence");
    assert_eq!(
        case.runs().await,
        vec![(2, afd_state::sql::RUN_STATUS_ACTIVE.to_owned())]
    );
    case.cleanup().await;
}
