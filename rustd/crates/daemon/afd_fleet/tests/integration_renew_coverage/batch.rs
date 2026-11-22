//! What one host's renewal frame decides before its statement runs: a fleet
//! whose stored ceiling will not parse is refused and its known lines logged,
//! a monthly ceiling is read beside the daily one, a fleet with no head
//! revision is renewed unchecked, a run whose stored posture no daemon writes
//! is refused alone, and a run with a durable task takes the per-run path.

use std::collections::HashMap;

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use af_wire::usage::{LineKind, RequestLines, RequestUsage};
use afd_fleet::run::renew::Renewals;
use jiff::Timestamp;

use super::{DEEP_DAILY_BUDGET, make_budget_unreadable, set_fleet_config};
use crate::report_seed::{DEEP_POOL, Held, held};
use crate::rows::Row;

/// The input tokens the runner's one reported request read.
const INPUT_TOKENS: u32 = 700;

/// A daily and a monthly ceiling, both with room.
const DEEP_MONTHLY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000","monthly_nanos":"30000000000000"}}}"#;

/// A renewal carrying one request the runner made since its last renewal.
fn one_request() -> RenewRequest {
    RenewRequest {
        requests: RequestLines(vec![RequestUsage {
            seq: 0,
            at: af_core::clock::now(),
            input_tokens: INPUT_TOKENS,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 0,
        }]),
    }
}

/// A held run whose payer and fleet have room.
async fn funded() -> Held {
    let held = held().await;
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
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

/// The model lines logged against the held run, as each line's input tokens.
async fn model_tokens(held: &Held) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT input_token_count FROM billing.usage_lines \
         WHERE work_id = $1 AND kind = $2 ORDER BY seq",
    )
    .bind(&held.issued.run_id)
    .bind(LineKind::Model.as_str())
    .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the run's model lines")
}

// Value: protects=a ceiling the daemon cannot read is not one the frame may
// ignore, and the usage a refused run reported is billed; fails_when=the frame
// admits an unparsable ceiling, answers it as an outage, or drops the refused
// run's request lines; why_new=the batch's unreadable-ceiling arm and its known
// lines had no case; seam=renew_batch over live Postgres, the head revision's
// budget corrupted.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_refuses_a_fleet_whose_ceiling_will_not_parse_and_logs_its_lines() {
    let held = funded().await;
    make_budget_unreadable(&held).await;
    let before = deadline(&held).await;

    let answer = frame(&held, one_request()).await;

    assert_eq!(
        answer.expect_err("an unreadable ceiling refuses").code(),
        error_code::RUN_BUDGET_EXCEEDED
    );
    assert_eq!(deadline(&held).await, before, "the run is not extended");
    assert_eq!(model_tokens(&held).await, [i64::from(INPUT_TOKENS)]);
    held.fixtures.cleanup().await;
}

// Value: protects=a fleet's monthly ceiling is read beside its daily one, and a
// fleet with room under both renews; fails_when=the monthly ceiling is dropped
// from the frame's read, or a declared monthly ceiling refuses a run with room;
// why_new=the monthly arm had no batch case; seam=renew_batch over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_renews_a_fleet_with_room_under_both_its_ceilings() {
    let held = funded().await;
    set_fleet_config(&held, DEEP_MONTHLY_BUDGET).await;

    let renewed = frame(&held, RenewRequest::default())
        .await
        .expect("a fleet with room renews");

    assert_eq!(deadline(&held).await, Some(renewed));
    held.fixtures.cleanup().await;
}

// Value: protects=a fleet whose head revision is gone is renewed unchecked, as an
// unreadable revision is, rather than refused; fails_when=a missing head revision
// refuses the run or faults the frame; why_new=the no-revision arm had no case;
// seam=renew_batch over live Postgres, the fleet's head revision cleared.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_frame_renews_a_fleet_with_no_head_revision_unchecked() {
    let held = funded().await;
    alter(
        &held,
        "UPDATE fleet.fleets SET head_revision_id = NULL WHERE id = $1",
        &held.fleet,
    )
    .await;

    let renewed = frame(&held, RenewRequest::default())
        .await
        .expect("a fleet with no head revision renews");

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
async fn a_frame_sends_a_run_with_a_task_to_the_per_run_path() {
    let held = funded().await;
    sqlx::query("UPDATE thread.runs SET task_id = $2 WHERE id = $1")
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
