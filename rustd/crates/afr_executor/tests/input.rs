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

/// A timeout long enough for a mebibyte to reach the executor on a loaded
/// machine before the process is killed, and far short of its own run.
const BRIEF: Duration = Duration::from_secs(2);

/// Far more than a pipe or a terminal buffers, so a write of it blocks until
/// the process reads.
fn flood() -> Bytes {
    Bytes::from(vec![b'x'; MIB])
}

#[tokio::test]
async fn a_large_write_to_a_process_that_never_reads_does_not_hold_its_timeout() {
    let harness = start().await;
    let spawn = Spawn::program("sleep").arg("30").timeout(BRIEF);
    let process = harness.client.spawn(&spawn).await.unwrap();
    let started = Instant::now();

    harness.client.write(process.id, flood()).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.endings, [Ending::TimedOut]);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_write_after_the_process_closed_its_input_is_refused_as_closed() {
    let harness = start().await;
    let process = harness
        .client
        .spawn(&Spawn::program("sh").args(["-c", "exec 0<&-; sleep 30"]))
        .await
        .unwrap();

    // The first writes may land before the shell closes its input; one after
    // is refused once the writer has met the closed pipe.
    let mut refused = None;
    for _write in 0..50 {
        match harness
            .client
            .write(process.id, Bytes::from_static(b"x\n"))
            .await
        {
            Ok(()) => tokio::time::sleep(Duration::from_millis(20)).await,
            Err(failure) => {
                refused = Some(failure);
                break;
            }
        }
    }
    harness.client.kill(process.id).await.unwrap();
    finish(process).await;

    let refused = refused.unwrap();
    assert!(refused_with(&refused, BACKLOG_FULL), "{refused}");
    assert!(refused.to_string().contains("input is closed"), "{refused}");
}

#[tokio::test]
async fn writes_past_the_queue_are_refused_and_a_kill_still_lands() {
    let harness = start().await;
    let process = harness
        .client
        .spawn(&Spawn::program("sleep").arg("30"))
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
    assert_eq!(finished.endings, [Ending::Signaled(15)]);
}

#[tokio::test]
async fn a_terminal_process_that_never_reads_is_still_stopped_by_its_timeout() {
    let harness = start().await;
    let spawn = Spawn::program("sleep").arg("30").terminal().timeout(BRIEF);
    let process = harness.client.spawn(&spawn).await.unwrap();

    harness.client.write(process.id, flood()).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.endings, [Ending::TimedOut]);
}
