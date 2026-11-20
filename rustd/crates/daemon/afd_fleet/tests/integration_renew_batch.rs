//! One host's renewals in one statement: each run answered on
//! its own, every write under its own fence, and the frame's statements
//! counted.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::{ErrorCode, RUN_EXCEEDED_MAX_RUNTIME, RUN_LOST};
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_wire::report::RenewRequest;
use af_wire::usage::LineKind;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::run::renew::Renewals;
use jiff::Timestamp;

use crate::report_seed::{Held, held_in};
use crate::rows::Row;
use crate::support::Fixtures;

/// The seeded fleet's daily ceiling (`fleet_run_reads.rs`).
pub(crate) const SEEDED_DAILY_NANOS: i64 = 1_000_000_000;

/// The run's runtime lines, as `(seq, runtime_ms)`.
const RUNTIME_LINES: &str = "SELECT seq, runtime_ms FROM billing.usage_lines \
     WHERE work_id = $1 AND kind = $2 ORDER BY seq";

/// One spend line on `fleet`, charged `nanos` now: what the fleet's budget reads.
const SPEND: &str = "INSERT INTO billing.usage_lines \
     (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name, turn_id, \
      provider, model, posture, requested_at, input_token_count, cached_input_token_count, \
      cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count, \
      output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at, updated_at) \
     VALUES (uuidv7(), 'fleet_run', uuidv7(), $5, 0, $1, $2, $3, 'spend', uuidv7(), \
             'fixture', 'fixture', 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, $4, now(), now())";

/// Renews `runs` on `held`'s host in one frame, on a fresh run service.
pub(crate) async fn frame(
    held: &Held,
    runs: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<Timestamp>> {
    frame_on(&held.fixtures.plane(), held, runs, funds_due).await
}

/// [`frame`], on `plane`: a long-lived service keeps its caches between frames.
async fn frame_on(
    plane: &afd_fleet::run::RunService,
    held: &Held,
    runs: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<Timestamp>> {
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            runs: runs
                .iter()
                .map(|run| ((*run).clone(), RenewRequest::default()))
                .collect(),
            funds_due,
            now: held.now,
            kept: std::collections::HashMap::new(),
        })
        .await
}

/// Each answer's refusal code, or `None` where it extended.
pub(crate) fn codes(answers: &[afd_fleet::Result<Timestamp>]) -> Vec<Option<ErrorCode>> {
    answers
        .iter()
        .map(|answer| answer.as_ref().err().map(afd_fleet::Error::code))
        .collect()
}

async fn runtime_lines(held: &Held, run: &Uuid7) -> Vec<(i64, i64)> {
    sqlx::query_as(RUNTIME_LINES)
        .bind(run)
        .bind(LineKind::Runtime.as_str())
        .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("runtime lines")
}

pub(crate) async fn spend(
    held: &Held,
    tenant: &Uuid7,
    workspace: &Uuid7,
    fleet: &Uuid7,
    nanos: i64,
) {
    sqlx::query(SPEND)
        .bind(tenant)
        .bind(workspace)
        .bind(fleet)
        .bind(nanos)
        .bind(LineKind::Model.as_str())
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the spend line writes");
}

// Value: protects=one statement renews a host's live runs and refuses the
// rest each on its own; fails_when=a refused run blocks or extends another,
// or the frame costs more than its pre-read and statement once caches are warm;
// why_new=renewal was about ten statements a run; seam=none; no_claim=a
// run with a durable task, which keeps the per-run path.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_extends_live_refuses_fenced() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let reassigned = held.hold_another().await;
    let capped = held.hold_another().await;
    held.fixtures
        .lapse_holder(&reassigned.run, &reassigned.fleet)
        .await;
    crate::seed::select_fleet(&held.runs, &held.spare, &reassigned.fleet)
        .await
        .expect("the spare is reassigned the lapsed fleet");
    held.fixtures
        .age(
            Row::Run(&capped.run),
            "created_at",
            af_core::timing::MAX_RUNTIME,
        )
        .await;
    held.run_a_slice().await;
    let runs = [&held.issued.run_id, &reassigned.run, &capped.run];
    // Warms the price and definition caches, so the counted frame is a
    // steady-state tick's.
    let plane = held.fixtures.plane();
    let _warm = frame_on(&plane, &held, &runs, false).await;
    held.run_a_slice().await;

    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    let window = Counted::open(&owner).await;
    let log = Capture::install();
    let answers = frame_on(&plane, &held, &runs, false).await;
    let ran = window.close().await;
    let refusals: Vec<_> = log
        .events()
        .into_iter()
        .filter(|line| line.field("event") == Some("renew_batch_refused"))
        .map(|line| {
            (
                line.field("run_id").map(str::to_owned),
                line.field("verdict").map(str::to_owned),
            )
        })
        .collect();
    drop(log);

    assert_eq!(
        codes(&answers),
        [None, Some(RUN_LOST), Some(RUN_EXCEEDED_MAX_RUNTIME)],
        "{answers:?}"
    );
    assert_eq!(
        refusals,
        [
            (
                Some(reassigned.run.as_str().to_owned()),
                Some("lost".to_owned())
            ),
            (
                Some(capped.run.as_str().to_owned()),
                Some("max_runtime".to_owned())
            ),
        ],
        "each refusal the per-run path answers silently is named once"
    );
    let heads: Vec<String> = ran
        .statements()
        .iter()
        .map(|(text, calls)| format!("{calls}x {}", text.chars().take(90).collect::<String>()))
        .collect();
    assert_eq!(ran.total(), 2, "the pre-read and the statement: {heads:#?}");
    assert_eq!(
        runtime_lines(&held, &held.issued.run_id).await.len(),
        2,
        "two slices billed"
    );
    assert!(
        runtime_lines(&held, &reassigned.run).await.is_empty(),
        "the reassigned run bills nothing"
    );
    assert!(
        runtime_lines(&held, &capped.run).await.is_empty(),
        "the capped run bills nothing"
    );
    owner.close().await;
    held.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn single_and_batch_renewals_obey_the_saved_runtime_cap()
-> Result<(), Box<dyn std::error::Error>> {
    let held = crate::report_seed::held().await;
    let run = &held.issued.run_id;
    sqlx::query("UPDATE thread.runs SET max_runtime_seconds=60, created_at=clock_timestamp()-interval '50 seconds' WHERE id=$1")
        .bind(run).execute(&mut *held.fixtures.database.acquire().await?).await?;
    let started = held
        .fixtures
        .instant(Row::Run(run), "created_at")
        .await
        .ok_or("saved run creation time")?;
    let expected = af_core::clock::after(started, std::time::Duration::from_secs(60));
    let renewed = crate::integration_renew_clamp::extend(
        &held.runs,
        run,
        &held.runner,
        (&held.run_key, &held.pricing),
        held.now,
    )
    .await?;
    assert_eq!(
        crate::integration_renew_clamp::deadline(&renewed),
        Some(expected)
    );
    let batch = crate::integration_renew_batch::frame(&held, &[run], false)
        .await
        .into_iter()
        .next()
        .ok_or("one batch answer")??;
    assert_eq!(batch, expected);
    held.fixtures.cleanup().await;
    Ok(())
}
