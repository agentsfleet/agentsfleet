//! A poll whose peeks the datastore refuses: the one that still answers
//! serves the poll, and only when both fail does the poll fail.
//!
//! Each test overwrites a partition's hash with a string, which every hash
//! command refuses, and restores it before cleaning up.

use std::time::{Duration, Instant};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture as Trace;
use afd_core::timing::READY_HINT_DEADLINE_MS;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::streams::fleet_stream_key;
use afd_fleet::lease::Leases;
use afd_observability::metrics::label::fleet::ReadyHint;
use afd_observability::test_util::Capture;

use super::scene::{partition, scene};
use super::{CMD_DEL, CMD_SET, HINT_FAILURES, KIND, NOT_A_HASH};
use crate::fake_redis::{FakeRedis, Reply};
use crate::requests::ENROLLED_AT;
use crate::seed::select_within_one_rotation;
use crate::support::Fixtures;

/// How long the fake node's peeks may wait before the client gives up.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

/// A directory naming partition 5, as the fake answers `HGETALL`.
const DIRECTORY_NAMING_FIVE: &str =
    "*2\r\n$1\r\n5\r\n$36\r\n0190a5d0-0000-7000-8000-000000000005\r\n";

/// The runner the deadline poll is made for; no row backs it.
const DEADLINE_RUNNER: &str = "01890a5d-ac96-774b-bcce-b302099a80a1";

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

/// A directory that cannot be read or written costs the hint and nothing else:
/// the mark lands, both failures are counted, and the rotation leases the
/// fleet. The one write is this test's own line: the counters are the whole
/// process's, and a sibling test breaking a directory moves them too.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_unusable_directory_falls_through_to_the_rotation() {
    let capture = Capture::install();
    let log = Trace::install();
    let scene = scene("broken").await;
    let fleet = scene.fleet(&[]).await;
    let partition = fleet.partition().index().to_string();
    let directory = scene.directory_key();
    scene.raw(&directory, CMD_SET, Some(NOT_A_HASH)).await;
    let writes = capture.sum(HINT_FAILURES, &[(KIND, ReadyHint::Write.as_str())]);
    let reads = capture.sum(HINT_FAILURES, &[(KIND, ReadyHint::Read.as_str())]);

    scene.send(&fleet).await;
    assert!(
        scene
            .index
            .token_for(&fleet.id)
            .await
            .expect("readable")
            .is_some(),
        "the mark landed though its entry did not"
    );
    let leased = select_within_one_rotation(
        &scene.leases,
        &fleet.runners[0],
        UnixMillis::from_millis(ENROLLED_AT),
    )
    .await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id)
    );
    let unwritten = log.only("ready_hint_write_failed");
    assert_eq!(unwritten.field("partition"), Some(partition.as_str()));
    assert!(
        capture.sum(HINT_FAILURES, &[(KIND, ReadyHint::Write.as_str())]) > writes,
        "the write is counted"
    );
    assert!(
        capture.sum(HINT_FAILURES, &[(KIND, ReadyHint::Read.as_str())]) > reads,
        "and every read"
    );
    scene.raw(&directory, CMD_DEL, None).await;
    scene.fixtures.cleanup().await;
}

/// The node holding the rotation's partition and the hinted one accepts their
/// peeks and never answers: the rotation peek waits out the request timeout,
/// and the hinted peek after it gives up at the hint deadline rather than
/// waiting that timeout out a second time.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_hint_deadline_falls_through() {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("HGETALL", Reply::Raw(DIRECTORY_NAMING_FIVE)),
        ("HRANDFIELD", Reply::Silent),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_TIMEOUT);
    let queue = Dragonfly::connect(&config)
        .await
        .expect("a fake that answers PONG is accepted");
    let fixtures = Fixtures::create().await;
    let leases = Leases::new(fixtures.database.clone(), queue, Entropy::new());
    let runner = Uuid7::parse(DEADLINE_RUNNER).expect("a runner id");
    let log = Trace::install();
    let started = Instant::now();

    let (polled, _cost) = leases
        .select_measured(&runner, &[], UnixMillis::from_millis(ENROLLED_AT))
        .await;

    let elapsed = started.elapsed();
    assert!(polled.is_err(), "neither peek answered");
    let refused = log.only("assign_ready_hint_peek_failed");
    let gave_up = format!("within {READY_HINT_DEADLINE_MS}ms");
    assert!(
        refused
            .field("reason")
            .is_some_and(|why| why.contains(&gave_up)),
        "the hinted peek gave up at the hint deadline: {:?}",
        refused.fields
    );
    // Two request timeouts would take twice REQUEST_TIMEOUT; one and the
    // hint's deadlines stay well under one and a half.
    assert!(
        elapsed < REQUEST_TIMEOUT + REQUEST_TIMEOUT / 2,
        "one request timeout and the hint's deadlines, not two timeouts: {elapsed:?}"
    );
}
