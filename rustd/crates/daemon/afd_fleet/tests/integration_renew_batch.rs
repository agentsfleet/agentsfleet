//! One host's renewals in one statement: each lease answered on
//! its own, every write under its own fence, and the frame's statements
//! counted.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::{ErrorCode, RUN_LEASE_EXCEEDED_MAX_RUNTIME, RUN_LEASE_LOST};
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_wire::report::RenewRequest;
use af_wire::usage::LineKind;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::lease::renew::Renewals;
use jiff::Timestamp;

use crate::report_seed::{Held, held_in};
use crate::rows::Row;
use crate::support::Fixtures;

/// The seeded fleet's daily ceiling (`fleet_lease_reads.rs`).
pub(crate) const SEEDED_DAILY_NANOS: i64 = 1_000_000_000;

/// The lease's runtime lines, as `(seq, runtime_ms)`.
const RUNTIME_LINES: &str = "SELECT seq, runtime_ms FROM billing.usage_lines \
     WHERE work_id = $1 AND kind = $2 ORDER BY seq";

/// One spend line on `fleet`, charged `nanos` now: what the fleet's budget reads.
const SPEND: &str = "INSERT INTO billing.usage_lines \
     (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id, \
      provider, model, posture, requested_at, input_token_count, cached_input_token_count, \
      cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count, \
      output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at, updated_at) \
     VALUES (uuidv7(), 'fleet_lease', uuidv7(), $5, 0, $1, $2, $3, 'spend', uuidv7(), \
             'fixture', 'fixture', 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, $4, now(), now())";

/// Renews `leases` on `held`'s host in one frame, on a fresh lease service.
pub(crate) async fn frame(
    held: &Held,
    leases: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<Timestamp>> {
    frame_on(&held.fixtures.plane(), held, leases, funds_due).await
}

/// [`frame`], on `plane`: a long-lived service keeps its caches between frames.
async fn frame_on(
    plane: &afd_fleet::lease::LeaseService,
    held: &Held,
    leases: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<Timestamp>> {
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            leases: leases
                .iter()
                .map(|lease| ((*lease).clone(), RenewRequest::default()))
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

async fn runtime_lines(held: &Held, lease: &Uuid7) -> Vec<(i64, i64)> {
    sqlx::query_as(RUNTIME_LINES)
        .bind(lease)
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

// Value: protects=one statement renews a host's live leases and refuses the
// rest each on its own; fails_when=a refused lease blocks or extends another,
// or the frame costs more than its pre-read and statement once caches are warm;
// why_new=renewal was about ten statements a lease; seam=none; no_claim=a
// lease with a durable task, which keeps the per-lease path.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_extends_live_refuses_fenced() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let reclaimed = held.hold_another().await;
    let capped = held.hold_another().await;
    held.fixtures
        .lapse_holder(&reclaimed.lease, &reclaimed.fleet)
        .await;
    crate::seed::select_fleet(&held.leases, &held.spare, &reclaimed.fleet)
        .await
        .expect("the spare reclaims the lapsed fleet");
    held.fixtures
        .age(
            Row::Lease(&capped.lease),
            "created_at",
            af_core::timing::MAX_RUNTIME,
        )
        .await;
    held.run_a_slice().await;
    let leases = [&held.issued.lease_id, &reclaimed.lease, &capped.lease];
    // Warms the price and definition caches, so the counted frame is a
    // steady-state tick's.
    let plane = held.fixtures.plane();
    let _warm = frame_on(&plane, &held, &leases, false).await;
    held.run_a_slice().await;

    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    let window = Counted::open(&owner).await;
    let log = Capture::install();
    let answers = frame_on(&plane, &held, &leases, false).await;
    let ran = window.close().await;
    let refusals: Vec<_> = log
        .events()
        .into_iter()
        .filter(|line| line.field("event") == Some("renew_batch_refused"))
        .map(|line| {
            (
                line.field("lease_id").map(str::to_owned),
                line.field("verdict").map(str::to_owned),
            )
        })
        .collect();
    drop(log);

    assert_eq!(
        codes(&answers),
        [
            None,
            Some(RUN_LEASE_LOST),
            Some(RUN_LEASE_EXCEEDED_MAX_RUNTIME)
        ],
        "{answers:?}"
    );
    assert_eq!(
        refusals,
        [
            (
                Some(reclaimed.lease.as_str().to_owned()),
                Some("lost".to_owned())
            ),
            (
                Some(capped.lease.as_str().to_owned()),
                Some("max_runtime".to_owned())
            ),
        ],
        "each refusal the per-lease path answers silently is named once"
    );
    let heads: Vec<String> = ran
        .statements()
        .iter()
        .map(|(text, calls)| format!("{calls}x {}", text.chars().take(90).collect::<String>()))
        .collect();
    assert_eq!(ran.total(), 2, "the pre-read and the statement: {heads:#?}");
    assert_eq!(
        runtime_lines(&held, &held.issued.lease_id).await.len(),
        2,
        "two slices billed"
    );
    assert!(
        runtime_lines(&held, &reclaimed.lease).await.is_empty(),
        "the reclaimed lease bills nothing"
    );
    assert!(
        runtime_lines(&held, &capped.lease).await.is_empty(),
        "the capped lease bills nothing"
    );
    owner.close().await;
    held.fixtures.cleanup().await;
}
