//! A poll repeating the key of a lease its worker never read frees that
//! lease at once; a host refusing its claim hands the fleet straight back.

use afd_core::id::Uuid7;
use afd_db::test_util::mint_uuid7;
use afd_fleet::lease::{Billed, Committed, Delivery, Issue, Kind};
use afd_observability::metrics::label::fleet::IssueDenied;

use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::held;
use crate::rows::{LAPSE, Row};
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, claim_fleet, seeded};
use crate::support::Fixtures;

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
struct Keyed {
    fixtures: Fixtures,
    runner: Uuid7,
    fleet: String,
    event_id: String,
    lease_id: String,
    key: Uuid7,
}

async fn keyed_lease() -> Keyed {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        event_id,
    } = seeded::<1>(&fixtures).await;
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
/// its key, the lease is let go with no expiry counted, and the next poll —
/// under a fresh key — takes the same admission back at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lost_lease_reply_is_freed_by_the_repeated_key() {
    let lost = keyed_lease().await;
    let expired_before = expired_count(&lost.fixtures, &lost.runner).await;

    let answer = lost
        .fixtures
        .plane()
        .poll(
            &lost.runner,
            &[],
            false,
            lost.fixtures.enrolled_at(),
            None,
            Some(lost.key.clone()),
        )
        .await
        .expect("the repeat is answered")
        .into_body();
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
        "no work, poll again now: {answer}"
    );
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

/// A host already running its one worker claims a second
/// fleet; issue answers `Denied(Capacity)`, writes no lease, and the slot it
/// hands back is claimed by another runner on its next poll.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_capacity_denial_releases_the_slot() {
    let busy = keyed_lease().await;
    let Seeded {
        runners: [other],
        fleet: second,
        tenant,
        ..
    } = seeded::<1>(&busy.fixtures).await;
    let tag = crate::requests::placement_tag(&second);
    sqlx::query(
        "UPDATE runner.runners SET labels = labels || to_jsonb($2::text) WHERE id = $1::uuid",
    )
    .bind(busy.runner.as_str())
    .bind(&tag)
    .execute(
        &mut *busy
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the busy runner may run the second fleet too");
    let leases = busy.fixtures.leases();
    let claimed = claim_fleet(&leases, &busy.runner, &second)
        .await
        .expect("claimed");
    let tenant_id = Uuid7::parse(&tenant).expect("a seeded tenant");

    let issued = leases
        .issue(
            &busy.runner,
            &claimed,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            busy.fixtures.enrolled_at(),
        )
        .await
        .expect("the issue runs");
    leases
        .release(&claimed.fleet_id, claimed.fence)
        .await
        .expect("the denied claim is handed back, as the pass does");
    let taken = claim_fleet(&leases, &other, &second).await;

    assert_eq!(issued, Issue::Denied(IssueDenied::Capacity));
    assert_eq!(
        taken.map(|acquired| acquired.event_id),
        Some(claimed.event_id),
        "another runner takes the fleet on its next poll"
    );
    busy.fixtures.cleanup().await;
}

/// A runner killed after issue stops renewing; past its
/// deadline another runner takes the admission back as a repeat, and the
/// dead runner's late report is fenced out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_sigkill_mid_lease_reclaims_once_and_refuses_the_late_report() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load runs")
        .expect("the lease is the runner's");
    held.fixtures.lapse_holder(lease_id, &held.fleet).await;

    let reclaimed = claim_fleet(&held.leases, &held.spare, &held.fleet)
        .await
        .expect("the lapsed admission is taken back");
    let again = claim_fleet(&held.leases, &held.runner, &held.fleet).await;
    let late = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            held.now,
        ))
        .await
        .expect("a fenced report is an answer");

    assert_eq!(
        (
            reclaimed.event_id.as_str(),
            reclaimed.delivery,
            reclaimed.kind
        ),
        (held.event_id.as_str(), Delivery::Repeat, Kind::Reclaim)
    );
    assert!(again.is_none(), "reclaimed once: the live holder keeps it");
    assert!(
        matches!(late, Committed::Fenced),
        "the dead holder's report is refused"
    );
    held.fixtures.cleanup().await;
}
