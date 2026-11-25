//! What one host's renewal frame decides before its statement runs: a funded
//! run renews, a run whose stored posture no daemon writes is refused alone,
//! and a run with a durable task takes the per-run path.

use std::collections::HashMap;

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use afd_fleet::run::renew::Renewals;
use jiff::Timestamp;

use crate::report_seed::{DEEP_POOL, Held, held};
use crate::rows::Row;

/// A held run whose payer has room.
async fn funded() -> Held {
    let held = held().await;
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    held
}

/// The held run renewed alone in one frame, carrying `request`.
async fn frame(held: &Held, request: RenewRequest) -> afd_fleet::Result<Timestamp> {
    let mut answers = held
        .fixtures
        .plane()
        .renew_batch(Renewals {
            runner_id: &held.runner,
            runs: vec![(held.issued.run_id.clone(), request)],
            funds_due: false,
            now: held.now,
            kept: HashMap::new(),
        })
        .await;
    assert_eq!(answers.len(), 1, "one answer per run");
    answers.remove(0)
}

/// Runs `statement` with `row` as `$1`, expecting it to touch one row.
async fn alter(held: &Held, statement: &'static str, row: &Uuid7) {
    let changed = sqlx::query(statement)
        .bind(row)
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the fixture row changes");
    assert_eq!(changed.rows_affected(), 1, "{statement}");
}

async fn deadline(held: &Held) -> Option<Timestamp> {
    held.fixtures
        .instant(Row::Run(&held.issued.run_id), "run_expires_at")
        .await
}

// Value: protects=a funded run renews in its host's frame, its deadline
// extended to the frame's answer; fails_when=the frame refuses or faults a run
// whose payer has room; seam=renew_batch over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_renews_a_funded_run() {
    let held = funded().await;

    let renewed = frame(&held, RenewRequest::default())
        .await
        .expect("a funded run renews");

    assert_eq!(deadline(&held).await, Some(renewed));
    held.fixtures.cleanup().await;
}

// Value: protects=a run whose stored posture no daemon writes is refused alone,
// never priced as another posture; fails_when=the frame prices an unknown posture,
// or its fault fails the whole frame; why_new=the frame's pricing-fault arm had no
// case; seam=renew_batch over live Postgres, the run's posture rewritten.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_refuses_a_run_whose_posture_no_daemon_writes() {
    let held = funded().await;
    alter(
        &held,
        "UPDATE thread.runs SET posture = 'bartered' WHERE id = $1",
        &held.issued.run_id,
    )
    .await;
    let before = deadline(&held).await;

    let answer = frame(&held, RenewRequest::default()).await;

    assert_eq!(
        answer.expect_err("an unknown posture is refused").code(),
        error_code::INTERNAL_DB_QUERY
    );
    assert_eq!(deadline(&held).await, before, "the run is not extended");
    held.fixtures.cleanup().await;
}

// Value: protects=a run that runs a durable task never renews through the frame's
// statement, which does not check task authority; fails_when=the frame keeps a
// tasked run in its statement; why_new=the frame's task triage had no case;
// seam=renew_batch over live Postgres, the run pointed at a task no row holds,
// answered as the per-run path answers it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_sends_a_run_with_a_durable_task_to_the_per_run_path() {
    let held = funded().await;
    sqlx::query("UPDATE thread.runs SET durable_task_id = $2 WHERE id = $1")
        .bind(&held.issued.run_id)
        .bind(afd_db::test_util::mint_id())
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the run names a task");

    let batched = frame(&held, RenewRequest::default()).await;
    let alone = held
        .fixtures
        .plane()
        .renew(
            &held.runner,
            &held.issued.run_id,
            RenewRequest::default(),
            held.now,
        )
        .await;

    assert!(
        alone.is_err(),
        "the per-run path refuses a task no row holds: {alone:?}"
    );
    assert_eq!(
        batched.as_ref().err().map(afd_fleet::Error::code),
        alone.as_ref().err().map(afd_fleet::Error::code),
        "the frame answers as the per-run path does: {batched:?} {alone:?}"
    );
    held.fixtures.cleanup().await;
}
