//! The report's pricing: the closing cost counts the statement's own lines, its
//! runtime line matches the rater, a report under the wrong pricing facts
//! writes nothing, and a warm replica reads no price book.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_wire::usage::UsageLine;
use afd_billing::price::{PriceBook, Rater, Window};
use afd_billing::rates::Posture;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::lease::Committed;
use afd_fleet::lease::usage::Pricing;

use crate::integration_report_statement::{reported, runtime_lines};
use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::{held, held_in};
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn closing_cost_counts_the_reports_own_lines() {
    const LOGGED: i64 = 500;
    let held = held().await;
    sqlx::query(
        "INSERT INTO billing.usage_lines (id, work_type, work_id, seq, kind, tenant_id, workspace_id, \
         fleet_id, fleet_name, event_id, provider, model, posture, requested_at, \
         input_token_count, cached_input_token_count, cache_write_token_count, \
         cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms, \
         unpriced_token_count, charged_nanos, created_at, updated_at) \
         SELECT uuidv7(), $2, l.id, 7, $4::text, l.tenant_id, l.workspace_id, l.fleet_id, 'Support triage', \
                l.event_id, l.provider, l.model, l.posture, now(), 0, 0, 0, 0, 0, 0, 0, 0, $3, now(), now() \
         FROM runner.leases l WHERE l.id = $1",
    )
    .bind(&held.issued.lease_id)
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(LOGGED)
    .bind(af_wire::usage::LineKind::Model.as_str())
    .execute(&mut *held.fixtures.database.acquire().await.expect("biller"))
    .await
    .expect("an earlier line of the event");
    let before = afd_events::fleet_counters(&held.fixtures.database, &held.fleet)
        .await
        .expect("counters");
    let lease = reported(&held).await;
    let at = af_core::clock::after(held.now, crate::report_seed::SLICE);

    let Committed::Settled {
        charged, closed, ..
    } = held
        .leases
        .commit_report(report(
            &held.issued.lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            at,
        ))
        .await
        .expect("the report commits")
    else {
        unreachable!("the lease is the runner's and live");
    };

    let closed = closed.expect("the event closed");
    assert!(charged.as_i64() > 0, "the report logged a runtime line");
    assert_eq!(closed.row.cost_nanos, Some(LOGGED + charged.as_i64()));
    assert_eq!(
        closed.counters.budget_used_nanos,
        before.budget_used_nanos + charged.as_i64(),
        "the budget moved by the report's own lines"
    );
    held.fixtures.cleanup().await;
}

// Value: protects=statement pricing equals the rater; fails_when=the runtime
// line's number, charge or tier drifts from what the rater writes;
// why_new=the runtime line is priced inside the statement; seam=none;
// no_claim=the batch and expiry statements, which their own suites price.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn statement_lines_match_rater() {
    let held = held().await;
    let lease = reported(&held).await;
    let at = af_core::clock::after(held.now, crate::report_seed::SLICE);
    held.leases
        .commit_report(report(
            &held.issued.lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            at,
        ))
        .await
        .expect("the report commits");

    let [(seq, runtime_ms, charged, price)] = runtime_lines(&held)
        .await
        .try_into()
        .expect("one runtime line");
    let book = PriceBook::load(
        &mut *held.fixtures.database.acquire().await.expect("reader"),
        PROVIDER,
        MODEL,
    )
    .await
    .expect("the book reads");
    let line = UsageLine::runtime(
        u32::try_from(seq).expect("a slice start"),
        held.now,
        u32::try_from(runtime_ms).expect("a slice's milliseconds"),
    );
    let rated = Rater::new(
        &book,
        Posture::Platform,
        Window {
            from: lease.lease_created_at,
            until: af_core::clock::now(),
        },
    )
    .rate(&line)
    .expect("the line rates");
    assert_eq!(seq, 0, "the lease's first slice starts with the lease");
    assert_eq!(charged, rated.charged.as_i64());
    assert_eq!(price, rated.price);
    held.fixtures.cleanup().await;
}

// Value: protects=a wrong pricing guess writes nothing; fails_when=lines are
// logged against another model; why_new=the statement guards on the facts it
// was priced at; seam=none; no_claim=a guess that names the right model.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn report_with_wrong_pricing_facts_writes_nothing() {
    let held = held().await;
    let lease = reported(&held).await;
    let wrong = Pricing::load(
        &held.fixtures.database,
        Posture::Platform,
        PROVIDER,
        "another-model",
        held.issued.lease_id.as_str(),
    )
    .await;
    let at = af_core::clock::after(held.now, crate::report_seed::SLICE);

    let committed = held
        .leases
        .commit_report(report(
            &held.issued.lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &wrong),
            RESPONSE_ACCEPTED,
            at,
        ))
        .await
        .expect("the report commits");

    assert!(
        matches!(committed, Committed::Settled { .. }),
        "{committed:?}"
    );
    let models: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT model FROM billing.usage_lines WHERE work_id = $1")
            .bind(&held.lease_key)
            .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("models");
    assert_eq!(
        models,
        [MODEL],
        "the retry priced under the lease's own model"
    );
    assert_eq!(runtime_lines(&held).await.len(), 1, "settled once");
    held.fixtures.cleanup().await;
}

/// A fragment of the price book read, `afd_billing`'s `select_price_book.sql`.
const PRICE_BOOK_READ: &str = "p.input_nanos_per_mtok";

// Value: protects=a warm replica's report reads no price book; fails_when=the
// report loads its book past the version-keyed cache; why_new=the report's
// book moved into the cache; seam=none; no_claim=a catalogue edit between
// the two, which the statement's version guard and its reprice answer.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_warm_replica_report_reads_no_price_book() {
    let held = held_in(Fixtures::create_isolated().await).await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    let plane = held.fixtures.plane();
    let request = crate::integration_held_sandbox::report::processed(&held, None);
    let mut untimed = af_observability::verb_span::VerbClock::untimed();

    let cold = Counted::open(&owner).await;
    plane
        .report_with(&held.runner, &request, held.now, None, &mut untimed)
        .await
        .expect("the report settles the lease");
    let cold = cold.close().await;
    let warm = Counted::open(&owner).await;
    plane
        .report_with(&held.runner, &request, held.now, None, &mut untimed)
        .await
        .expect("the replay answers the stored outcome");
    let warm = warm.close().await;

    assert_eq!(
        cold.calls_containing(PRICE_BOOK_READ),
        1,
        "{:?}",
        cold.statements()
    );
    assert_eq!(
        warm.calls_containing(PRICE_BOOK_READ),
        0,
        "{:?}",
        warm.statements()
    );
    drop(plane);
    owner.close().await;
    held.fixtures.cleanup().await;
}
