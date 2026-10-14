//! The claim path's commands, counted: a poll's tally carries the drop of an
//! entry no envelope can be read from, and the type check and create of a
//! consumer group that vanished, beside the reads either side of them.

use afd_dragonfly::FleetStreams;
use afd_dragonfly::streams::{FLEET_CONSUMER_GROUP, fleet_stream_key};

use super::scene::{partition, scene};
use crate::queue;
use crate::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON};

const CMD_XGROUP: &str = "XGROUP";
/// A consumer that read the fleet's entry and never acknowledged it.
const VANISHED_CONSUMER: &str = "consumer-that-vanished";
const XGROUP_DESTROY: &str = "DESTROY";

/// An entry no envelope can be read from is acknowledged away, and the poll
/// counts that command with the others it sent.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_undecodable_entry_counts_its_drop() {
    let scene = scene("undecodable").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    queue::enqueue_cutover_era(
        scene.fixtures.queue(),
        &fleet.id,
        &fleet.workspace,
        ACTOR,
        EVENT_TYPE,
        REQUEST_JSON,
    )
    .await;
    scene.mark(&fleet).await;

    let (none, cost) = scene.poll(&fleet.runners[0]).await;

    assert!(none.is_none(), "the fleet's only entry cannot be leased");
    assert_eq!(
        cost.dragonfly_commands, 5,
        "rotation, rung peek, takeover, new read, drop: {cost:?}"
    );
    assert_eq!(
        cost.database_roundtrips, 2,
        "the candidate scan and the one claim: {cost:?}"
    );
    scene.fixtures.cleanup().await;
}

/// A consumer group that vanished is recreated where the ledgers say delivery
/// stopped, and the poll counts the type check and the create beside the
/// takeover that found it gone and the reads that follow.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_restored_group_counts_its_type_check_and_create() {
    let scene = scene("restored").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    let key = scene
        .fixtures
        .queue()
        .physical_key(&fleet_stream_key(&fleet.id));
    let mut destroy = redis::cmd(CMD_XGROUP);
    destroy
        .arg(XGROUP_DESTROY)
        .arg(&key)
        .arg(FLEET_CONSUMER_GROUP);
    let _: i64 = scene
        .fixtures
        .queue()
        .command(CMD_XGROUP, &key, &destroy)
        .await
        .expect("destroy the group");

    let (leased, cost) = scene.poll(&fleet.runners[0]).await;

    let leased = leased.expect("the restored group hands the event over");
    assert_eq!(leased.fleet_id.as_str(), fleet.id);
    assert!(!leased.redelivered, "a new read is not owed");
    assert_eq!(
        cost.dragonfly_commands, 7,
        "rotation, rung peek, the takeover that found no group, type check, \
         create, takeover, new read: {cost:?}"
    );
    scene.fixtures.cleanup().await;
}

/// An entry another consumer read and never acknowledged is owed, not new:
/// the claim takes it over and leases it as redelivered, which keeps whatever
/// it waited on out of `queue_wait_ms`.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_owed_entry_is_leased_as_redelivered() {
    let scene = scene("owed").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    FleetStreams::new(scene.fixtures.queue().clone())
        .read_new(&fleet.id, VANISHED_CONSUMER)
        .await
        .expect("the stream answers")
        .expect("the other consumer is handed the entry");

    let (leased, _cost) = scene.poll(&fleet.runners[0]).await;

    let leased = leased.expect("the owed entry is taken over and leased");
    assert!(leased.redelivered, "an entry taken over is owed, not new");
    scene.fixtures.cleanup().await;
}
