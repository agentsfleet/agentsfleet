//! A poll repeating the key of a lease its worker never read frees that
//! lease at once, and only the runner whose poll it named can free it.

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::mint_uuid7;
use afd_fleet::lease::{Asked, Billed, Delivery, Kind};
use afd_observability::test_util::Capture as Metrics;
use afd_wire::lease::AcceptedKinds;

use crate::rows::{LAPSE, Row};
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, claim_fleet, seeded};
use crate::support::Fixtures;

use super::fence;

/// The runner's lifetime count of leases that lapsed.
async fn expired_count(fixtures: &Fixtures, runner: &Uuid7) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT expired_count FROM runner.lifetime_counters WHERE runner_id = $1::uuid",
    )
    .bind(runner.as_str())
    .fetch_optional(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the tally reads")
}

/// One issued lease on a seeded fleet, written under `key` as a worker's poll
/// carrying it would have.
pub(super) struct Keyed {
    pub(super) fixtures: Fixtures,
    pub(super) runner: Uuid7,
    pub(super) fleet: String,
    pub(super) event_id: String,
    pub(super) lease_id: String,
    pub(super) key: Uuid7,
}

/// Counted each time a repeated key lets a lease go.
const REPLAYED: &str = "agentsfleet_lease_poll_replayed_total";

pub(super) async fn keyed_lease() -> Keyed {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        event_id,
    } = seeded::<1>(&fixtures).await;
    keyed_on(fixtures, runner, fleet, tenant, event_id).await
}

/// [`keyed_lease`] for a fleet and runner already seeded.
async fn keyed_on(
    fixtures: Fixtures,
    runner: Uuid7,
    fleet: String,
    tenant: String,
    event_id: String,
) -> Keyed {
    let key = mint_uuid7();
    let leases = fixtures.leases().with_poll_key(Some(key.clone()));
    let acquired = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("claimed");
    let tenant_id = Uuid7::parse(&tenant).expect("a seeded tenant");
    let issued = leases
        .issue(
            &runner,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            fixtures.enrolled_at(),
        )
        .await
        .expect("the issue runs")
        .issued()
        .expect("the lease is written");
    Keyed {
        fixtures,
        runner,
        fleet,
        event_id,
        lease_id: issued.lease_id.as_str().to_owned(),
        key,
    }
}

/// The reply to an issued lease is lost; the worker repeats
/// its key through the route's claim, the lease is let go with no expiry
/// counted, the replay is counted, and the next poll — under a fresh key —
/// takes the same admission back at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lost_lease_reply_is_freed_by_the_repeated_key() {
    let lost = keyed_lease().await;
    let expired_before = expired_count(&lost.fixtures, &lost.runner).await;
    let metrics = Metrics::install();
    let replayed_before = metrics.sum(REPLAYED, &[]);
    let plane = lost.fixtures.plane();
    let drafts = afd_authoring::Drafts::new(
        lost.fixtures.database.clone(),
        Entropy::new(),
        std::sync::Arc::clone(&plane.biller),
    );

    let answer = plane
        .claim(
            &drafts,
            &lost.runner,
            Asked {
                held: &[],
                accepted: AcceptedKinds::FLEET,
                key: Some(&lost.key),
            },
            false,
            lost.fixtures.enrolled_at(),
        )
        .await
        .expect("the repeat is answered");
    let replayed = metrics.sum(REPLAYED, &[]) - replayed_before;
    let freed = (
        lost.fixtures
            .text(Row::Lease(&lost.lease_id), "status")
            .await,
        lost.fixtures.slot_is_free(&lost.fleet).await,
        expired_count(&lost.fixtures, &lost.runner).await,
    );
    let again = claim_fleet(&lost.fixtures.leases(), &lost.runner, &lost.fleet)
        .await
        .expect("the admission is claimable at once");

    assert!(
        answer.contains("\"lease\":null") && answer.contains("\"retry_after_ms\":0"),
        "no work, poll again now, through the route's claim: {answer}"
    );
    assert_eq!(replayed, 1, "the freed lease is counted once");
    assert_eq!(
        freed,
        (
            Some(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED.to_owned()),
            true,
            expired_before,
        ),
        "the lease is let go, the slot freed, and no expiry is counted"
    );
    assert_eq!(
        (again.event_id.as_str(), again.delivery, again.kind),
        (lost.event_id.as_str(), Delivery::Repeat, Kind::Reclaim)
    );
    lost.fixtures.cleanup().await;
}

/// A key whose lease already ended answers no work and writes
/// nothing, even with the fleet's work ready.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_key_of_an_ended_lease_answers_no_work() {
    let ended = keyed_lease().await;
    sqlx::query("UPDATE runner.leases SET status = $2 WHERE id = $1::uuid")
        .bind(&ended.lease_id)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_REPORTED)
        .execute(
            &mut *ended
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the lease reported");
    ended
        .fixtures
        .age(Row::Slot(&ended.fleet), "lease_expires_at", LAPSE)
        .await;
    let fence_before = ended
        .fixtures
        .value::<i64>(Row::Slot(&ended.fleet), "fencing_token")
        .await;

    let answer = ended
        .fixtures
        .plane()
        .poll(
            &ended.runner,
            &[],
            false,
            ended.fixtures.enrolled_at(),
            None,
            Some(ended.key.clone()),
        )
        .await
        .expect("the repeat is answered")
        .into_body();

    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert_eq!(
        (
            ended
                .fixtures
                .value::<i64>(Row::Slot(&ended.fleet), "fencing_token")
                .await,
            ended
                .fixtures
                .text(Row::Lease(&ended.lease_id), "status")
                .await,
        ),
        (
            fence_before,
            Some(afd_fleet::lease::sql::LEASE_STATUS_REPORTED.to_owned()),
        ),
        "no claim ran and the ended lease is untouched"
    );
    ended.fixtures.cleanup().await;
}

/// A key names one runner's poll: another runner sending it frees nothing,
/// and the first runner's lease and fence stand.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_key_frees_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        tenant,
        event_id,
    } = seeded::<2>(&fixtures).await;
    let held = keyed_on(fixtures, holder, fleet, tenant, event_id).await;
    let fence_before = fence(&held.fixtures, &held.fleet).await;

    let answer = held
        .fixtures
        .plane()
        .poll(
            &other,
            &[],
            false,
            held.fixtures.enrolled_at(),
            None,
            Some(held.key.clone()),
        )
        .await
        .expect("the other runner is answered")
        .into_body();

    assert!(
        !answer.contains("\"retry_after_ms\":0"),
        "the key is not the other runner's to replay: {answer}"
    );
    assert_eq!(
        (
            held.fixtures
                .text(Row::Lease(&held.lease_id), "status")
                .await,
            fence(&held.fixtures, &held.fleet).await,
            held.fixtures.slot_is_free(&held.fleet).await,
        ),
        (
            Some(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE.to_owned()),
            fence_before,
            false,
        ),
        "the holder's lease, fence and slot stand"
    );
    held.fixtures.cleanup().await;
}
