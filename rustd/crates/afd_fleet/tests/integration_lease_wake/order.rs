//! Which dirty partition a poll is sent to when the directory names several:
//! the oldest first from a fresh cursor, then the oldest few in turn as the
//! cursor moves, so consecutive polls do not all race for one partition.

use afd_fleet::lease::Acquired;

use super::scene::{partition, scene};

/// The fleet a poll leased, if it leased one.
fn leased(acquired: Option<Acquired>) -> Option<String> {
    acquired.map(|acquired| acquired.fleet_id.to_string())
}

/// Two fleets marked in two partitions are leased by two workers, one poll
/// each, oldest mark first.
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
    assert_eq!(
        scene.dirty().await,
        [first.partition().index(), second.partition().index()],
        "oldest mark first"
    );

    let (one, _) = scene.poll(&first.runners[0]).await;
    let (two, _) = scene.poll(&second.runners[0]).await;

    assert_eq!(leased(one), Some(first.id));
    assert_eq!(leased(two), Some(second.id));
    scene.fixtures.cleanup().await;
}

/// Three fleets marked in turn are leased by three polls, the second of which
/// is sent past the oldest entry left. Each fleet's runners serve only that
/// fleet, so a poll sent to the wrong partition would lease nothing and spend
/// the entry another fleet's poll needed.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_consecutive_polls_take_the_oldest_entries_in_turn() {
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
    assert_eq!(
        scene.dirty().await,
        [&first, &second, &third].map(|fleet| fleet.partition().index()),
        "oldest mark first"
    );

    // The cursor stands at 0: the first poll takes the oldest of three and
    // spends it. At 1 the second takes the second of the two left, which an
    // oldest-only hint would never send it to.
    let (one, _) = scene.poll(&first.runners[0]).await;
    let (two, _) = scene.poll(&third.runners[0]).await;
    let (three, _) = scene.poll(&second.runners[0]).await;

    assert_eq!(leased(one), Some(first.id));
    assert_eq!(leased(two), Some(third.id), "sent past the oldest entry");
    assert_eq!(leased(three), Some(second.id));
    scene.fixtures.cleanup().await;
}
