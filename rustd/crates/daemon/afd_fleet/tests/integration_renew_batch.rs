//! One host's renewals in one statement (M245 §7): each lease answered on
//! its own, every write under its own fence, and the frame's statements
//! counted.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::{
    ErrorCode, RUN_BUDGET_EXCEEDED, RUN_LEASE_CANCELLED, RUN_LEASE_EXCEEDED_MAX_RUNTIME,
    RUN_LEASE_LOST, RUN_LEASE_NOT_FOUND, RUN_LEASE_RENEWAL_NO_CREDITS,
};
use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::lease::renew::Renewals;
use jiff::Timestamp;

use crate::report_seed::{Held, held, held_in};
use crate::rows::Row;
use crate::support::Fixtures;

/// The seeded fleet's daily ceiling (`fleet_lease_reads.rs`).
const SEEDED_DAILY_NANOS: i64 = 1_000_000_000;

/// The lease's runtime lines, as `(seq, runtime_ms)`.
const RUNTIME_LINES: &str = "SELECT seq, runtime_ms FROM billing.usage_lines \
     WHERE work_id = $1 AND kind = 'runtime' ORDER BY seq";

/// One spend line on `fleet`, charged `nanos` now: what the fleet's budget reads.
const SPEND: &str = "INSERT INTO billing.usage_lines \
     (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id, \
      provider, model, posture, requested_at, input_token_count, cached_input_token_count, \
      cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count, \
      output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at, updated_at) \
     VALUES (uuidv7(), 'fleet_lease', uuidv7(), 'model', 0, $1, $2, $3, 'spend', uuidv7(), \
             'fixture', 'fixture', 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, $4, now(), now())";

/// Renews `leases` on `held`'s host in one frame, on a fresh lease service.
async fn frame(
    held: &Held,
    leases: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<(Timestamp, afd_billing::Nanos)>> {
    frame_on(&held.fixtures.plane(), held, leases, funds_due).await
}

/// [`frame`], on `plane`: a long-lived service keeps its caches between frames.
async fn frame_on(
    plane: &afd_fleet::lease::LeaseService,
    held: &Held,
    leases: &[&Uuid7],
    funds_due: bool,
) -> Vec<afd_fleet::Result<(Timestamp, afd_billing::Nanos)>> {
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
fn codes(answers: &[afd_fleet::Result<(Timestamp, afd_billing::Nanos)>]) -> Vec<Option<ErrorCode>> {
    answers
        .iter()
        .map(|answer| answer.as_ref().err().map(afd_fleet::Error::code))
        .collect()
}

async fn runtime_lines(held: &Held, lease: &Uuid7) -> Vec<(i64, i64)> {
    sqlx::query_as(RUNTIME_LINES)
        .bind(lease)
        .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("runtime lines")
}

async fn spend(held: &Held, tenant: &Uuid7, workspace: &Uuid7, fleet: &Uuid7, nanos: i64) {
    sqlx::query(SPEND)
        .bind(tenant)
        .bind(workspace)
        .bind(fleet)
        .bind(nanos)
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
    let answers = frame_on(&plane, &held, &leases, false).await;
    let ran = window.close().await;

    assert_eq!(
        codes(&answers),
        [
            None,
            Some(RUN_LEASE_LOST),
            Some(RUN_LEASE_EXCEEDED_MAX_RUNTIME)
        ],
        "{answers:?}"
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

// Value: protects=a lease another runner holds is refused alone;
// fails_when=one foreign id refuses the frame; why_new=the frame is one
// statement now; seam=none; no_claim=a malformed id, which the gateway refuses
// before the batch.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_answers_foreign_lease_lost_alone() {
    let held = held().await;
    let sibling = held.hold_another().await;
    let foreign = crate::report_seed::held().await;

    let answers = frame(
        &held,
        &[
            &held.issued.lease_id,
            &foreign.issued.lease_id,
            &sibling.lease,
        ],
        false,
    )
    .await;

    assert_eq!(
        codes(&answers),
        [None, Some(RUN_LEASE_NOT_FOUND), None],
        "{answers:?}"
    );
    foreign.fixtures.cleanup().await;
    held.fixtures.cleanup().await;
}

// Value: protects=a fleet at its ceiling stops alone, and one nano under runs
// on; fails_when=the statement compares as `>` rather than `budget::covers`'s
// reached, or one fleet's ceiling stops another; why_new=the ceiling is read
// in the statement; seam=none; no_claim=a monthly ceiling, which the seeded
// setup does not declare.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_refuses_over_budget_fleet_alone() {
    let held = held().await;
    let under = held.hold_another().await;
    let workspace = held.acquired.workspace_id.clone();
    spend(
        &held,
        &held.tenant,
        &workspace,
        &held.fleet,
        SEEDED_DAILY_NANOS,
    )
    .await;
    spend(
        &held,
        &under.tenant,
        &under.workspace,
        &under.fleet,
        SEEDED_DAILY_NANOS - 1,
    )
    .await;

    let answers = frame(&held, &[&held.issued.lease_id, &under.lease], false).await;

    assert_eq!(
        codes(&answers),
        [Some(RUN_BUDGET_EXCEEDED), None],
        "{answers:?}"
    );
    held.fixtures.cleanup().await;
}

// Value: protects=a short payer's leases stop on a checking tick and run on
// between; fails_when=the check runs every tick or never; why_new=funds are
// checked once per payer every third tick; seam=none; no_claim=the cadence
// itself, which `funds_due`'s unit test proves.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_short_payer_stops_on_a_checking_tick_alone() {
    let held = held().await;
    let funded = held.hold_another().await;
    held.fixtures.seed_wallet(&held.tenant, 0).await;
    let leases = [&held.issued.lease_id, &funded.lease];

    let between = frame(&held, &leases, false).await;
    let checking = frame(&held, &leases, true).await;

    assert_eq!(
        codes(&between),
        [None, None],
        "no check between ticks: {between:?}"
    );
    assert_eq!(
        codes(&checking),
        [Some(RUN_LEASE_RENEWAL_NO_CREDITS), None],
        "{checking:?}"
    );
    held.fixtures.cleanup().await;
}

// Value: protects=a Kill mark stops its lease alone and keeps its deadline;
// fails_when=the cancel check leaves the statement, or extends the lease
// anyway; why_new=the mark is read in the batch; seam=none; no_claim=the
// mark's delivery, which M243's suites prove.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_refuses_cancel_marked_lease() {
    let held = held().await;
    let sibling = held.hold_another().await;
    sqlx::query("UPDATE runner.leases SET cancel_requested_at = now() WHERE id = $1")
        .bind(&held.issued.lease_id)
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the Kill mark writes");
    let before = held
        .fixtures
        .instant(Row::Lease(&held.issued.lease_id), "lease_expires_at")
        .await;

    let answers = frame(&held, &[&held.issued.lease_id, &sibling.lease], false).await;

    assert_eq!(
        codes(&answers),
        [Some(RUN_LEASE_CANCELLED), None],
        "{answers:?}"
    );
    assert_eq!(
        held.fixtures
            .instant(Row::Lease(&held.issued.lease_id), "lease_expires_at")
            .await,
        before,
        "a cancelled lease keeps its deadline"
    );
    held.fixtures.cleanup().await;
}

/// [`frame_on`], with the facts a session kept from each lease's offer.
async fn kept_frame(
    plane: &afd_fleet::lease::LeaseService,
    held: &Held,
    lease: &Uuid7,
) -> afd_fleet::Result<(Timestamp, afd_billing::Nanos)> {
    let facts = held
        .leases
        .load_for_report(lease, &held.runner)
        .await
        .expect("the lease reads")
        .expect("the lease is the runner's");
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            leases: vec![(lease.clone(), RenewRequest::default())],
            funds_due: false,
            now: held.now,
            kept: std::collections::HashMap::from([(lease.clone(), facts)]),
        })
        .await
        .into_iter()
        .next()
        .expect("one answer per lease")
}

/// How many times the frame's pre-read ran in `ran`.
fn pre_reads(ran: &statements::Ran) -> u64 {
    ran.calls_containing("l.task_id, l.created_at, f.head_revision_id")
}

// Value: protects=a steady-state frame whose session kept its leases' facts
// runs only the renewal statement, and a moved catalogue sends the lease to
// the per-lease path once and the next frame back to the pre-read;
// fails_when=the frame still pre-reads, or a stale version keeps every later
// frame on the per-lease path; why_new=M245 §9 took the pre-read's facts from
// the session and the replica; seam=none; no_claim=a durable task's lease,
// which keeps no facts.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn kept_facts_skip_the_pre_read_until_a_version_moves() {
    let held = held_in(Fixtures::create_isolated().await).await;
    // A priced model: a book with no prices guards no catalogue version.
    held.fixtures
        .seed_model_rate(crate::seed::PROVIDER, crate::seed::MODEL)
        .await;
    let lease = &held.issued.lease_id;
    let plane = held.fixtures.plane();
    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;

    let learning = Counted::open(&owner).await;
    kept_frame(&plane, &held, lease)
        .await
        .expect("the first frame renews");
    let learning = learning.close().await;
    held.run_a_slice().await;
    let steady = Counted::open(&owner).await;
    kept_frame(&plane, &held, lease)
        .await
        .expect("a kept frame renews");
    let steady = steady.close().await;

    sqlx::query(
        "UPDATE model.catalogue_version SET resource_version = resource_version + 1 WHERE id = 1",
    )
    .execute(&mut *owner.acquire().await.expect("owner"))
    .await
    .expect("the catalogue moves");
    held.run_a_slice().await;
    kept_frame(&plane, &held, lease)
        .await
        .expect("a moved frame renews on the per-lease path");
    held.run_a_slice().await;
    let relearning = Counted::open(&owner).await;
    kept_frame(&plane, &held, lease)
        .await
        .expect("the next frame renews");
    let relearning = relearning.close().await;

    assert_eq!(pre_reads(&learning), 1, "an unknown version is read");
    assert_eq!(pre_reads(&steady), 0, "{:?}", steady.statements());
    assert_eq!(pre_reads(&relearning), 1, "a moved version is read again");
    owner.close().await;
    held.fixtures.cleanup().await;
}
