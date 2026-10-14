//! A poll whose peeks or claim the datastore refuses: the peek that still
//! answers serves the poll, only when both fail does the poll fail, and a
//! rung partition that faulted is left clear so later polls are not sent
//! back to the same fault.
//!
//! Each test overwrites a key with a string, which every hash and stream
//! command refuses, and restores it before cleaning up.

use afd_core::clock::UnixMillis;
use afd_dragonfly::streams::fleet_stream_key;

use super::scene::{partition, scene};
use super::{CMD_DEL, CMD_SET, NOT_A_HASH, RUNG_PEEK_FAILURES};
use crate::requests::ENROLLED_AT;

/// The cursor's partition cannot be read, but the doorbell rang another: the
/// rung peek serves the poll and its fleet is leased.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_broken_rotation_partition_leaves_the_rung_one() {
    let scene = scene("broken-rotation").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    let rotation = scene.partition_key(partition(0));
    scene.raw(&rotation, CMD_SET, Some(NOT_A_HASH)).await;

    let (leased, _cost) = scene.poll(&fleet.runners[0]).await;
    scene.raw(&rotation, CMD_DEL, None).await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id.clone()),
        "the rung partition still answered"
    );
    scene.fixtures.cleanup().await;
}

/// Both the cursor's partition and the rung one refuse their peeks: nothing
/// answered, so the poll fails rather than claiming no work.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_both_peeks_refused_fail_the_poll() {
    let _counted_alone = RUNG_PEEK_FAILURES.lock().await;
    let scene = scene("broken-both").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    let rotation = scene.partition_key(partition(0));
    let rung = scene.partition_key(fleet.partition());
    scene.raw(&rotation, CMD_SET, Some(NOT_A_HASH)).await;
    scene.raw(&rung, CMD_SET, Some(NOT_A_HASH)).await;

    let (polled, _cost) = scene
        .leases
        .select_measured(&fleet.runners[0], &[], UnixMillis::from_millis(ENROLLED_AT))
        .await;
    scene.raw(&rotation, CMD_DEL, None).await;
    scene.raw(&rung, CMD_DEL, None).await;

    assert!(polled.is_err(), "no peek answered, so the poll cannot");
    assert!(
        !scene.rung(fleet.partition()),
        "the unreadable partition is left clear, so the next poll is not sent back"
    );
    scene.fixtures.cleanup().await;
}

/// Both peeks answer but the rung fleet's claim faults, its stream
/// unreadable: the poll fails and leaves the partition clear, so later polls
/// are not sent back to the same fault while the rotation still reaches it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_rung_claim_that_faults_leaves_the_ring_clear() {
    let scene = scene("broken-claim").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    let stream = scene
        .fixtures
        .queue()
        .physical_key(&fleet_stream_key(&fleet.id));
    scene.raw(&stream, CMD_SET, Some(NOT_A_HASH)).await;

    let (polled, _cost) = scene
        .leases
        .select_measured(&fleet.runners[0], &[], UnixMillis::from_millis(ENROLLED_AT))
        .await;
    scene.raw(&stream, CMD_DEL, None).await;

    assert!(polled.is_err(), "the rung fleet's stream read faulted");
    assert!(
        !scene.rung(fleet.partition()),
        "the faulting partition is left clear"
    );
    scene.fixtures.cleanup().await;
}
