//! A poll whose peeks the datastore refuses: the one that still answers
//! serves the poll, and only when both fail does the poll fail.
//!
//! Each test overwrites a partition's hash with a string, which every hash
//! command refuses, and restores it before cleaning up.

use afd_core::clock::UnixMillis;
use afd_dragonfly::streams::fleet_stream_key;

use super::scene::{partition, scene};
use super::{CMD_DEL, CMD_SET, NOT_A_HASH};
use crate::requests::ENROLLED_AT;

/// The cursor's partition cannot be read, but the directory's names another:
/// the hinted peek serves the poll and its fleet is leased.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_broken_rotation_partition_leaves_the_hint() {
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
        "the directory's partition still answered"
    );
    scene.fixtures.cleanup().await;
}

/// Both the cursor's partition and the one the directory names refuse their
/// peeks: nothing answered, so the poll fails rather than claiming no work.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_both_peeks_refused_fail_the_poll() {
    let scene = scene("broken-both").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;
    let rotation = scene.partition_key(partition(0));
    let hinted = scene.partition_key(fleet.partition());
    scene.raw(&rotation, CMD_SET, Some(NOT_A_HASH)).await;
    scene.raw(&hinted, CMD_SET, Some(NOT_A_HASH)).await;

    let (polled, _cost) = scene
        .leases
        .select_measured(&fleet.runners[0], &[], UnixMillis::from_millis(ENROLLED_AT))
        .await;
    scene.raw(&rotation, CMD_DEL, None).await;
    scene.raw(&hinted, CMD_DEL, None).await;

    assert!(polled.is_err(), "no peek answered, so the poll cannot");
    assert!(
        scene.dirty().await.is_empty(),
        "the unreadable hint is spent, so the next poll is not sent back"
    );
    scene.fixtures.cleanup().await;
}

/// Both peeks answer but the hinted fleet's claim faults, its stream
/// unreadable: the poll fails, and it spends the entry, so later polls are not
/// sent back to the same fault while the rotation still reaches the fleet.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_hinted_claim_that_faults_spends_its_entry() {
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

    assert!(polled.is_err(), "the hinted fleet's stream read faulted");
    assert!(
        scene.dirty().await.is_empty(),
        "the faulting partition's entry is spent"
    );
    scene.fixtures.cleanup().await;
}
