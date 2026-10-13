//! Which rung partition a poll takes when the doorbell holds several: the
//! first at or after its cursor, so consecutive polls take different ones
//! and each ring is answered once.

use afd_fleet::lease::Acquired;

use super::scene::{Fleet, partition, scene};

/// The fleet a poll leased, if it leased one.
fn leased(acquired: Option<Acquired>) -> Option<String> {
    acquired.map(|acquired| acquired.fleet_id.to_string())
}

/// `fleets` in the order their partitions sit after partition 0.
fn by_partition<'a>(fleets: &[&'a Fleet]) -> Vec<&'a Fleet> {
    let mut ordered = fleets.to_vec();
    ordered.sort_by_key(|fleet| fleet.partition());
    ordered
}

/// Two fleets marked in two partitions are leased by two workers, one poll
/// each: the first takes the ring nearest its cursor and the second the one
/// left.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_marked_fleets_lease_within_one_interval() {
    let scene = scene("two").await;
    let first = scene.fleet(&[partition(0), partition(1)]).await;
    let second = scene
        .fleet(&[partition(0), partition(1), first.partition()])
        .await;
    scene.send(&first).await;
    scene.send(&second).await;
    let [near, far] = by_partition(&[&first, &second])[..] else {
        unreachable!("two fleets");
    };

    let (one, _) = scene.poll(&near.runners[0]).await;
    let (two, _) = scene.poll(&far.runners[0]).await;

    assert_eq!(leased(one), Some(near.id.clone()));
    assert_eq!(leased(two), Some(far.id.clone()));
    scene.fixtures.cleanup().await;
}

/// Three fleets marked in three partitions are leased by three polls in a
/// row. Each fleet's runners serve only that fleet, so a poll that took the
/// wrong ring would lease nothing and leave another fleet's ring clear.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_consecutive_polls_take_different_rung_partitions() {
    let scene = scene("spread").await;
    let mut avoid = vec![partition(0), partition(1), partition(2)];
    let first = scene.fleet(&avoid).await;
    avoid.push(first.partition());
    let second = scene.fleet(&avoid).await;
    avoid.push(second.partition());
    let third = scene.fleet(&avoid).await;
    for fleet in [&first, &second, &third] {
        scene.send(fleet).await;
    }

    // The cursors stand at 0, 1 and 2, all below every rung partition, so
    // each poll takes the nearest ring the polls before it left.
    for fleet in by_partition(&[&first, &second, &third]) {
        let (polled, _) = scene.poll(&fleet.runners[0]).await;
        assert_eq!(leased(polled), Some(fleet.id.clone()));
    }
    scene.fixtures.cleanup().await;
}
