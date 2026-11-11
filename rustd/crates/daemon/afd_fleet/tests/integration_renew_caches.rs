//! A replica's renewal caches: the facts a session keeps, the
//! versions the replica last read, and the books and ceilings cached by them,
//! each obeying a moved head, catalogue or price as an uncached read would.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::RUN_BUDGET_EXCEEDED;
use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use af_wire::usage::LineKind;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::lease::renew::Renewals;
use jiff::Timestamp;

use crate::integration_renew_batch::{SEEDED_DAILY_NANOS, codes, frame, spend};
use crate::report_seed::{Held, held, held_in};
use crate::support::Fixtures;

/// [`frame_on`], with the facts a session kept from each lease's offer.
async fn kept_frame(
    plane: &afd_fleet::lease::LeaseService,
    held: &Held,
    lease: &Uuid7,
) -> afd_fleet::Result<Timestamp> {
    kept_frame_with(plane, held, lease, RenewRequest::default()).await
}

/// [`kept_frame`], carrying `request`'s lines.
async fn kept_frame_with(
    plane: &afd_fleet::lease::LeaseService,
    held: &Held,
    lease: &Uuid7,
    request: RenewRequest,
) -> afd_fleet::Result<Timestamp> {
    let facts = held
        .leases
        .load_for_report(lease, &held.runner)
        .await
        .expect("the lease reads")
        .expect("the lease is the runner's");
    plane
        .renew_batch(Renewals {
            runner_id: &held.runner,
            leases: vec![(lease.clone(), request)],
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
// frame on the per-lease path; why_new=the frame takes the pre-read's facts from
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

/// A fleet config whose daily ceiling is `nanos`.
fn daily_ceiling(nanos: i64) -> String {
    format!(
        r#"{{"name":"ceiling","x-agentsfleet":{{"triggers":[],"tools":[],"budget":{{"daily_nanos":"{nanos}"}}}}}}"#
    )
}

// Value: protects=a replica that cached a fleet's ceiling obeys a new head
// revision another plane applied, with no signal between them;
// fails_when=the kept frame judges the cached revision's ceiling;
// why_new=a frame renews from kept facts and the replica's last-read head;
// seam=none; no_claim=a new lease, whose claim returns the head's document.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn config_cache_follows_new_head_revision() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let lease = &held.issued.lease_id;
    let workspace = held.acquired.workspace_id.clone();
    held.fixtures
        .native_config(&held.fleet, &daily_ceiling(10 * SEEDED_DAILY_NANOS))
        .await;
    let plane_a = held.fixtures.plane();
    kept_frame(&plane_a, &held, lease)
        .await
        .expect("plane A renews under the ten-dollar revision");
    spend(
        &held,
        &held.tenant,
        &workspace,
        &held.fleet,
        5 * SEEDED_DAILY_NANOS,
    )
    .await;
    held.run_a_slice().await;
    kept_frame(&plane_a, &held, lease)
        .await
        .expect("five dollars spent is under ten");

    // Plane B's Save: the fleet's head moves to a one-dollar revision.
    held.fixtures
        .native_config(&held.fleet, &daily_ceiling(SEEDED_DAILY_NANOS))
        .await;
    held.run_a_slice().await;
    let judged = kept_frame(&plane_a, &held, lease).await;

    assert_eq!(
        judged.as_ref().err().map(afd_fleet::Error::code),
        Some(RUN_BUDGET_EXCEEDED),
        "{judged:?}"
    );
    held.fixtures.cleanup().await;
}

/// One request line of `output` output tokens, numbered `seq`.
fn output_line(held: &Held, seq: u32, output: u32) -> RenewRequest {
    RenewRequest {
        requests: af_wire::usage::RequestLines(vec![af_wire::usage::RequestUsage {
            seq,
            at: held.now,
            input_tokens: 0,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: output,
        }]),
    }
}

/// The charge of the model line numbered `seq` on `lease`.
async fn model_charge(held: &Held, lease: &Uuid7, seq: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT charged_nanos FROM billing.usage_lines WHERE work_id = $1 AND kind = $3 AND seq = $2",
    )
    .bind(lease)
    .bind(seq)
    .bind(LineKind::Model.as_str())
    .fetch_one(&mut *held.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the model line is logged")
}

// Value: protects=a catalogue edit reprices the next renewal on a replica
// that cached the old book; fails_when=the kept frame prices from the cached
// book and the guard lets it through; why_new=renewals price from the
// replica's last-read catalogue version; seam=none; no_claim=the per-lease
// path's statement count, which the rerun pays once.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn catalogue_cache_reprices_after_bump() {
    let held = held_in(Fixtures::create_isolated().await).await;
    held.fixtures
        .seed_model_rate(crate::seed::PROVIDER, crate::seed::MODEL)
        .await;
    let lease = &held.issued.lease_id;
    let plane = held.fixtures.plane();
    kept_frame_with(&plane, &held, lease, output_line(&held, 0, 1_000))
        .await
        .expect("tick 1 renews");
    let before = model_charge(&held, lease, 0).await;

    // An admin edit: the output rate doubles and the catalogue moves.
    let mut owner = held.fixtures.database.acquire().await.expect("owner");
    sqlx::query(
        "UPDATE model.platform_model_prices SET output_nanos_per_mtok = output_nanos_per_mtok * 2",
    )
    .execute(&mut *owner)
    .await
    .expect("the rate doubles");
    sqlx::query(
        "UPDATE model.catalogue_version SET resource_version = resource_version + 1 WHERE id = 1",
    )
    .execute(&mut *owner)
    .await
    .expect("the catalogue moves");
    drop(owner);
    held.run_a_slice().await;
    kept_frame_with(&plane, &held, lease, output_line(&held, 1, 1_000))
        .await
        .expect("tick 2 renews on the per-lease path");
    let after = model_charge(&held, lease, 1).await;

    assert!(before > 0, "tick 1 priced its line");
    assert_eq!(after, 2 * before, "tick 2 charged the edited rate");
    held.fixtures.cleanup().await;
}

// Value: protects=the batch prices its runtime line as the rater prices it;
// fails_when=the statement's in-SQL runtime pricing drifts from `Rater`;
// why_new=the batch half of the runtime pricing proof lives here with the meter;
// seam=none; no_claim=the expiry statement, which the billing sweep's tests
// price.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn batch_lines_match_rater() {
    let held = held().await;
    held.fixtures
        .seed_model_rate(crate::seed::PROVIDER, crate::seed::MODEL)
        .await;
    let lease = &held.issued.lease_id;
    let answers = frame(&held, &[lease], false).await;
    assert_eq!(codes(&answers), [None], "{answers:?}");

    let (seq, runtime_ms, charged, price): (i64, i64, i64, Option<Uuid7>) = sqlx::query_as(
        "SELECT seq, runtime_ms, charged_nanos, platform_model_price_id \
         FROM billing.usage_lines WHERE work_id = $1 AND kind = $2",
    )
    .bind(lease)
    .bind(LineKind::Runtime.as_str())
    .fetch_one(&mut *held.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("one runtime line");
    let book = afd_billing::price::PriceBook::load(
        &mut *held.fixtures.database.acquire().await.expect("reader"),
        crate::seed::PROVIDER,
        crate::seed::MODEL,
    )
    .await
    .expect("the book reads");
    let line = af_wire::usage::UsageLine::runtime(
        u32::try_from(seq).expect("a slice start"),
        held.now,
        u32::try_from(runtime_ms).expect("a slice's milliseconds"),
    );
    let rated = afd_billing::price::Rater::new(
        &book,
        afd_billing::rates::Posture::Platform,
        afd_billing::price::Window {
            from: held.now,
            until: af_core::clock::now(),
        },
    )
    .rate(&line)
    .expect("the line rates");

    assert!(price.is_some(), "the seeded model prices its runtime");
    assert_eq!(charged, rated.charged.as_i64());
    assert_eq!(price, rated.price);
    held.fixtures.cleanup().await;
}

// Value: protects=a price added to a model the replica cached as unpriced is
// charged on the next renewal; fails_when=the cached empty book names no
// catalogue version, the statement skips its price guard, and the tokens stay
// free; why_new=books are cached by version, empty ones included; seam=none;
// no_claim=a book whose read failed, which is never cached.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_price_added_after_an_empty_book_is_charged() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let lease = &held.issued.lease_id;
    let plane = held.fixtures.plane();
    kept_frame_with(&plane, &held, lease, output_line(&held, 0, 1_000))
        .await
        .expect("tick 1 renews unpriced");
    assert_eq!(model_charge(&held, lease, 0).await, 0, "no price yet");

    held.fixtures
        .seed_model_rate(crate::seed::PROVIDER, crate::seed::MODEL)
        .await;
    let mut owner = held.fixtures.database.acquire().await.expect("owner");
    sqlx::query(
        "UPDATE model.catalogue_version SET resource_version = resource_version + 1 WHERE id = 1",
    )
    .execute(&mut *owner)
    .await
    .expect("the catalogue moves");
    drop(owner);
    held.run_a_slice().await;
    kept_frame_with(&plane, &held, lease, output_line(&held, 1, 1_000))
        .await
        .expect("tick 2 renews");

    assert!(
        model_charge(&held, lease, 1).await > 0,
        "tick 2 charges the price the catalogue now holds"
    );
    held.fixtures.cleanup().await;
}
