//! A refusal ends in one fenced statement, and a pass that ends on a
//! retry frees its fleet behind the backoff.

use std::time::Duration;

use afd_billing::Accounts;
use afd_core::timing::{CLAIM_RETRY_BACKOFF, LEASE_TTL};
use afd_db::config::DbRole;
use afd_fleet::lease::admit::Refusal;
use afd_fleet::lease::{Delivery, Ended, Plane};

use super::{admit_at, created_at, fence, ready_at};
use crate::rows::Row;
use crate::seed::{claim_fleet, seeded_parts};
use crate::support::Fixtures;

/// A stored document the runtime parser accepts.
const READABLE_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;
/// An event type this build does not know: refused before any money is read.
const UNKNOWN_TYPE: &str = "fixture-unknown";
/// A type the pass admits as far as the payer read.
const CHAT: &str = "chat";
/// Calls of a statement in this database, found by a phrase only it carries.
const CALLS: &str = "SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
     WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
       AND query LIKE $1";
/// The refusal's one statement.
const END_REFUSED: &str = "%failure_detail = NULLIF($7%";
/// The release a lease-less ending would owe.
const RELEASE_UNLEASED: &str = "%THEN last_runner_id END%";
/// A refusal label the fixtures end an event with.
const ENDED: Refusal = Refusal::labelled("fixture_ended");

/// One admission of `event_type` on `fleet`, `offset` seconds in.
async fn admit_typed(
    fixtures: &Fixtures,
    fleet: &str,
    workspace: &str,
    event_type: &str,
    offset: u64,
) -> String {
    crate::queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        crate::seed::ACTOR,
        event_type,
        crate::seed::REQUEST_JSON,
        afd_core::clock::after(fixtures.enrolled_at(), Duration::from_secs(offset)),
    )
    .await
}

async fn calls(fixtures: &Fixtures, phrase: &str) -> i64 {
    sqlx::query_scalar(CALLS)
        .bind(phrase)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("an observer"))
        .await
        .expect("the statement counter reads")
}

/// The fixture plane over a payer ledger that never answers: every pass ends
/// on the payer gate's retry.
fn failing_payer(fixtures: &Fixtures) -> Plane {
    Plane {
        accounts: Accounts::new(afd_db::test_util::unreachable_db()),
        ..fixtures.plane()
    }
}

/// Seconds the slot's deadline lies past the database clock; negative when free.
async fn deadline_from_now(fixtures: &Fixtures, fleet: &str) -> f64 {
    fixtures
        .value::<f64>(
            Row::Slot(fleet),
            "EXTRACT(EPOCH FROM lease_expires_at - clock_timestamp())::float8",
        )
        .await
        .expect("the slot exists")
}

/// A refused event ends, settles and frees its fleet in one
/// statement; the next admission is ready in its place.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_refused_event_ends_in_one_statement() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    fixtures.native_config(&fleet, READABLE_CONFIG).await;
    let refused = admit_typed(&fixtures, &fleet, &workspace, UNKNOWN_TYPE, 0).await;
    let next = admit_at(&fixtures, &fleet, &workspace, 1).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut *owner.acquire().await.expect("owner"))
        .await
        .expect("statement counter");
    owner.close().await;
    let claimed = claim_fleet(&fixtures.leases(), &runner, &fleet)
        .await
        .expect("claimed");
    let (ends_before, releases_before) = (
        calls(&fixtures, END_REFUSED).await,
        calls(&fixtures, RELEASE_UNLEASED).await,
    );

    let answer = fixtures
        .plane()
        .lease_claimed(claimed, &runner, fixtures.enrolled_at())
        .await
        .expect("a refusal is a decision");

    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert_eq!(
        (
            calls(&fixtures, END_REFUSED).await - ends_before,
            calls(&fixtures, RELEASE_UNLEASED).await - releases_before,
        ),
        (1, 0),
        "one ending statement, and no separate release"
    );
    assert_eq!(
        (
            fixtures
                .text(Row::Event(&fleet, &refused), "status")
                .await
                .as_deref()
                .map(str::to_owned),
            fixtures
                .value::<bool>(Row::Admission(&refused), "settled_at IS NOT NULL")
                .await,
            fixtures.slot_is_free(&fleet).await,
            ready_at(&fixtures, &fleet).await,
        ),
        (
            Some(afd_core::event::status::GATE_BLOCKED.to_owned()),
            Some(true),
            true,
            created_at(&fixtures, &next).await,
        ),
        "ended, settled, free, and the next admission is ready"
    );
    fixtures.cleanup().await;
}

/// A pass that faults frees its fleet behind the backoff: a
/// poll inside it passes the fleet over, and one after it takes the same
/// admission back as a repeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_pass_fault_releases_the_slot_after_the_backoff() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    fixtures.native_config(&fleet, READABLE_CONFIG).await;
    let event = admit_typed(&fixtures, &fleet, &workspace, CHAT, 0).await;
    let claimed = claim_fleet(&fixtures.leases(), &runner, &fleet)
        .await
        .expect("claimed");

    let answer = failing_payer(&fixtures)
        .lease_claimed(claimed, &runner, fixtures.enrolled_at())
        .await
        .expect("a payer read that fails retries");
    let backing_off = deadline_from_now(&fixtures, &fleet).await;
    let inside = claim_fleet(&fixtures.leases(), &runner, &fleet).await;
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", CLAIM_RETRY_BACKOFF)
        .await;
    let after = claim_fleet(&fixtures.leases(), &runner, &fleet)
        .await
        .expect("after");

    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert!(
        backing_off > 0.0 && backing_off <= CLAIM_RETRY_BACKOFF.as_secs_f64(),
        "freed behind the backoff, not at once and not for a lease: {backing_off}s"
    );
    assert!(
        inside.is_none(),
        "a poll inside the backoff passes the fleet over"
    );
    assert_eq!(
        (after.event_id.as_str(), after.delivery),
        (event.as_str(), Delivery::Repeat)
    );
    fixtures.cleanup().await;
}

/// Two ready fleets, the older one's pass failing. Sequential
/// polls by one runner take the younger fleet next, and the older one again
/// only once its backoff has passed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_failing_fleet_does_not_starve_a_healthy_one() {
    let fixtures = Fixtures::create_with_queue().await;
    let (older, older_ws, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let younger = afd_db::test_util::mint_id();
    let tag = crate::requests::placement_tag(&older);
    fixtures
        .seed_fleet(&younger, &older_ws, &tenant, &tag)
        .await;
    for fleet in [&older, &younger] {
        fixtures.native_config(fleet, READABLE_CONFIG).await;
    }
    let _stuck = admit_typed(&fixtures, &older, &older_ws, CHAT, 0).await;
    let healthy = admit_typed(&fixtures, &younger, &older_ws, CHAT, 1).await;

    let first = failing_payer(&fixtures)
        .lease(&runner, &[], false, fixtures.enrolled_at())
        .await
        .expect("the older fleet's pass retries");
    let second = fixtures.leases().select(&runner, &[]).await.expect("poll");
    let third = fixtures.leases().select(&runner, &[]).await.expect("poll");
    fixtures
        .age(Row::Slot(&older), "lease_expires_at", CLAIM_RETRY_BACKOFF)
        .await;
    let fourth = fixtures.leases().select(&runner, &[]).await.expect("poll");

    assert!(first.contains("\"lease\":null"), "{first}");
    assert_eq!(
        second.map(|acquired| acquired.event_id),
        Some(healthy),
        "the younger fleet is next, not the failing one"
    );
    assert!(
        third.is_none(),
        "the failing fleet waits out its backoff: {third:?}"
    );
    assert_eq!(
        fourth.map(|acquired| acquired.fleet_id.as_str().to_owned()),
        Some(older),
        "after the backoff the older fleet is retried"
    );
    fixtures.cleanup().await;
}

/// A refusal under a fence the fleet has moved past reads
/// `Already` and changes no row.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_refusal_under_a_stale_fence_reads_already() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [first, second]) = seeded_parts::<2>(&fixtures).await;
    let event = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let leases = fixtures.leases();
    let stale = claim_fleet(&leases, &first, &fleet)
        .await
        .expect("first claim");
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", LEASE_TTL)
        .await;
    let live = claim_fleet(&leases, &second, &fleet)
        .await
        .expect("reclaimed");
    let fence_now = fence(&fixtures, &fleet).await;

    let ended = leases
        .block(&stale, ENDED)
        .await
        .expect("a stale ending runs");

    assert_eq!(ended, Ended::Already);
    assert_eq!(
        (
            fixtures.text(Row::Event(&fleet, &event), "status").await,
            fixtures
                .value::<bool>(Row::Admission(&event), "settled_at IS NOT NULL")
                .await,
            fence(&fixtures, &fleet).await,
            fixtures.slot_is_free(&fleet).await,
        ),
        (
            Some(afd_core::event::status::RECEIVED.to_owned()),
            Some(false),
            fence_now,
            false,
        ),
        "the event stays open, the admission unsettled, the live claim held"
    );
    assert!(live.fence > stale.fence);
    fixtures.cleanup().await;
}
