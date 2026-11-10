//! The report is one statement (M245 §6): every write a terminal report makes
//! commits together, under the lease's own row lock, fenced and replay-safe,
//! and its runtime line is priced as the rater prices it.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::usage::{DAEMON_SEQ_BASE, RequestUsage, UsageLine};
use afd_billing::price::{PriceBook, Rater, Window};
use afd_billing::rates::Posture;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};
use afd_fleet::lease::usage::Pricing;
use afd_fleet::lease::{Committed, Reported, TerminalReport};

use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::{Held, held, held_in};
use crate::rows::Row;
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// The lease's runtime lines: their number, milliseconds, charge and tier.
const RUNTIME_LINES: &str = "SELECT seq, runtime_ms, charged_nanos, platform_model_price_id \
    FROM billing.usage_lines WHERE work_id = $1 AND seq >= $2 ORDER BY seq";

/// What [`RUNTIME_LINES`] answers.
type RuntimeLine = (i64, i64, i64, Option<Uuid7>);

/// The address a fixture's answer is owed to.
const THREAD: &str = "C0FIXTURE:1700000000.000100";

async fn runtime_lines(held: &Held) -> Vec<RuntimeLine> {
    sqlx::query_as(RUNTIME_LINES)
        .bind(&held.lease_key)
        .bind(i64::from(DAEMON_SEQ_BASE))
        .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("runtime lines")
}

async fn reported(held: &Held) -> Reported {
    held.leases
        .load_for_report(&held.issued.lease_id, &held.runner)
        .await
        .expect("the lease reads")
        .expect("the lease is the runner's")
}

/// The seeded admission asks for its answer back on a Slack thread.
async fn owe_to_slack(held: &Held) {
    sqlx::query(
        "UPDATE fleet.admissions SET reply_provider = $2, reply_address = $3 WHERE id = $1",
    )
    .bind(&held.event_id)
    .bind(afd_connector::Provider::Slack.id())
    .bind(THREAD)
    .execute(&mut *held.fixtures.database.acquire().await.expect("producer"))
    .await
    .expect("the admission names a destination");
}

async fn scalar(held: &Held, statement: &'static str, id: &Uuid7) -> i64 {
    sqlx::query_scalar(statement)
        .bind(id)
        .fetch_one(&mut *held.fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("a count")
}

/// One request line the runner reports with the run's answer.
fn request(held: &Held) -> RequestUsage {
    RequestUsage {
        seq: 0,
        at: held.now,
        input_tokens: 1_000,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 200,
    }
}

fn with_requests<'a>(
    report: TerminalReport<'a>,
    requests: &'a [RequestUsage],
) -> TerminalReport<'a> {
    TerminalReport { requests, ..report }
}

// Value: protects=the report's statement count and its writes; fails_when=a
// write leaves the statement; why_new=the report was a dozen statements;
// seam=none; no_claim=a task's settlement, which follows it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn one_statement_report_commits_every_write() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    owe_to_slack(&held).await;
    let lease = reported(&held).await;
    let requests = [request(&held)];
    let at = af_core::clock::after(held.now, crate::report_seed::SLICE);

    let window = Counted::open(&owner).await;
    let committed = held
        .leases
        .commit_report(with_requests(
            report(
                &held.issued.lease_id,
                &held.runner,
                &lease,
                (&held.lease_key, &held.pricing),
                RESPONSE_ACCEPTED,
                at,
            ),
            &requests,
        ))
        .await
        .expect("the report commits");
    let ran = window.close().await;

    let Committed::Settled { closed, owed, .. } = committed else {
        unreachable!("the lease is the runner's and live");
    };
    assert_eq!(ran.total(), 1, "{:?}", ran.statements());
    assert_eq!(
        closed.expect("the event closed").row.status,
        af_core::event::status::PROCESSED
    );
    assert!(owed.is_some(), "the answer is owed to its thread");
    let checkpoint: String =
        sqlx::query_scalar("SELECT context_json::text FROM fleet.sessions WHERE fleet_id = $1")
            .bind(&held.fleet)
            .fetch_one(&mut *held.fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("the session is checkpointed");
    assert!(checkpoint.contains(crate::report_commit::RESUME_RESPONSE));
    let released: bool = sqlx::query_scalar(
        "SELECT lease_expires_at < now() FROM runner.fleet_slots WHERE fleet_id = $1",
    )
    .bind(&held.fleet)
    .fetch_one(&mut *held.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the slot reads");
    assert!(released, "the slot is released");
    let lines: i64 = scalar(
        &held,
        "SELECT count(*) FROM billing.usage_lines WHERE work_id = $1",
        &held.lease_key,
    )
    .await;
    assert_eq!(lines, 2, "the request line and the runtime line");
    let audits: i64 = scalar(
        &held,
        "SELECT count(*) FROM runner.events WHERE runner_id = $1 AND event_type = 'lease_released'",
        &held.runner,
    )
    .await;
    assert_eq!(audits, 1, "the released audit row");
    owner.close().await;
    held.fixtures.cleanup().await;
}

// Value: protects=a reclaimed lease's report writes nothing but its known
// request lines; fails_when=the fence leaves the statement's guard, or the
// refused statement's locks outlive it and the known lines wait on them;
// why_new=the guard moved into the one statement; seam=none; no_claim=what the
// known lines charge (blocker 4's own tests).
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn one_statement_report_refuses_stale_fence() {
    let held = held().await;
    let lease = reported(&held).await;
    let requests = [request(&held)];
    held.fixtures
        .lapse_holder(&held.issued.lease_id, &held.fleet)
        .await;
    crate::seed::select_fleet(&held.leases, &held.spare, &held.fleet)
        .await
        .expect("the spare reclaims the lapsed fleet");

    let committed = held
        .leases
        .commit_report(with_requests(
            report(
                &held.issued.lease_id,
                &held.runner,
                &lease,
                (&held.lease_key, &held.pricing),
                RESPONSE_ACCEPTED,
                held.now,
            ),
            &requests,
        ))
        .await
        .expect("the report answers");

    let Committed::Fenced { settlement } = committed else {
        unreachable!("the spare holds the fleet: {committed:?}");
    };
    assert!(settlement.is_some(), "the known request line is logged");
    assert!(runtime_lines(&held).await.is_empty(), "no runtime line");
    assert_eq!(
        held.fixtures
            .text(Row::Event(&held.fleet, &held.event_id), "status")
            .await
            .as_deref(),
        Some(af_core::event::status::RECEIVED),
        "the event row is unchanged"
    );
    held.fixtures.cleanup().await;
}

// Value: protects=a replayed report settles once; fails_when=two copies both
// pass the guard; why_new=the replay answer moved into the statement's
// trailing row; seam=none; no_claim=a replay after the lease row is purged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn one_statement_report_replay_returns_stored_outcome() {
    const COPIES: usize = 100;
    let held = held().await;
    owe_to_slack(&held).await;
    let lease = reported(&held).await;
    let at = af_core::clock::after(held.now, crate::report_seed::SLICE);

    let answers = futures_util::future::join_all((0..COPIES).map(|_| {
        held.leases.commit_report(report(
            &held.issued.lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            at,
        ))
    }))
    .await;

    let settled = answers
        .iter()
        .filter(|answer| matches!(answer, Ok(Committed::Settled { .. })))
        .count();
    let repeated = answers
        .iter()
        .filter(|answer| matches!(answer, Ok(Committed::AlreadySettled)))
        .count();
    assert_eq!((settled, repeated), (1, COPIES - 1));
    assert_eq!(runtime_lines(&held).await.len(), 1, "one runtime line");
    let obligations = scalar(
        &held,
        "SELECT count(*) FROM fleet.obligations WHERE event_id = $1",
        &held.event_id,
    )
    .await;
    assert_eq!(obligations, 1, "one obligation");
    held.fixtures.cleanup().await;
}

// Value: protects=the closing row's cost and counters; fails_when=the statement
// answers its snapshot, missing the lines it inserted; why_new=the close and
// the lines are one statement; seam=none; no_claim=a concurrent insert.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn closing_cost_counts_the_reports_own_lines() {
    const LOGGED: i64 = 500;
    let held = held().await;
    sqlx::query(
        "INSERT INTO billing.usage_lines (id, work_type, work_id, seq, tenant_id, workspace_id, \
         fleet_id, fleet_name, event_id, provider, model, posture, requested_at, \
         input_token_count, cached_input_token_count, cache_write_token_count, \
         cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms, \
         unpriced_token_count, charged_nanos, created_at, updated_at) \
         SELECT uuidv7(), $2, l.id, 7, l.tenant_id, l.workspace_id, l.fleet_id, 'Support triage', \
                l.event_id, l.provider, l.model, l.posture, now(), 0, 0, 0, 0, 0, 0, 0, 0, $3, now(), now() \
         FROM runner.leases l WHERE l.id = $1",
    )
    .bind(&held.issued.lease_id)
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(LOGGED)
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
// no_claim=the batch and expiry statements, which §7 rewrites.
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
        u32::try_from(seq).expect("a daemon number"),
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
    assert_eq!(seq, i64::from(DAEMON_SEQ_BASE), "the first daemon number");
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
