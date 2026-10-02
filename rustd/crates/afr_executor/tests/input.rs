//! Writes to a process that never reads: queued, then refused, and never in
//! the way of a timeout or a kill.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::{Duration, Instant};

use afr_executor::{Ending, Executor as _, Spawn};
use bytes::Bytes;

use crate::support::{BACKLOG_FULL, MIB, PATIENCE, finish, refused_with, start};

/// Far more than a pipe or a terminal buffers, so a write of it blocks until
/// the process reads.
fn flood() -> Bytes {
    Bytes::from(vec![b'x'; MIB])
}

#[tokio::test]
async fn a_large_write_to_a_process_that_never_reads_does_not_hold_its_timeout() {
    let harness = start().await;
    let spawn = Spawn::program("sleep")
        .arg("30")
        .timeout(Duration::from_millis(300));
    let process = harness.client.spawn(spawn).await.unwrap();
    let started = Instant::now();

    harness.client.write(process.id, flood()).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.endings, [(Ending::TimedOut, 0)]);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn writes_past_the_queue_are_refused_and_a_kill_still_lands() {
    let harness = start().await;
    let process = harness
        .client
        .spawn(Spawn::program("sleep").arg("30"))
        .await
        .unwrap();

    let mut refused = None;
    for _write in 0..64 {
        if let Err(failure) = harness.client.write(process.id, flood()).await {
            refused = Some(failure);
            break;
        }
    }
    let killed = tokio::time::timeout(PATIENCE, harness.client.kill(process.id)).await;
    let finished = finish(process).await;

    let refused = refused.unwrap();
    assert!(refused_with(&refused, BACKLOG_FULL), "{refused}");
    killed.unwrap().unwrap();
    assert_eq!(finished.endings, [(Ending::Signaled(15), 0)]);
}

#[tokio::test]
async fn a_terminal_process_that_never_reads_is_still_stopped_by_its_timeout() {
    let harness = start().await;
    let spawn = Spawn::program("sleep")
        .arg("30")
        .terminal()
        .timeout(Duration::from_millis(300));
    let process = harness.client.spawn(spawn).await.unwrap();

    harness.client.write(process.id, flood()).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.endings, [(Ending::TimedOut, 0)]);
}
