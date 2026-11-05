//! A host refusing its claim hands the fleet straight back, and a killed
//! runner's lease is reclaimed once with its late report fenced out.

use afd_core::id::Uuid7;
use afd_fleet::lease::{Billed, Committed, Delivery, Issue, Kind};
use afd_observability::metrics::label::fleet::IssueDenied;

use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::held;
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, claim_fleet, seeded};

use super::replay::keyed_lease;

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
