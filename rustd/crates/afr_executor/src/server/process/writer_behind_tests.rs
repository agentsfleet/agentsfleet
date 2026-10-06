//! A driver whose writer is behind still answers a stop and a deadline.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::tests::{PATIENCE, QUEUED, YES, started_within};

/// A deadline on a driver whose writer is behind lands at once: the process
/// is killed and its ending is `timed_out`, while the writer still has not
/// read; the drain then forwards the held chunk and the ending.
#[tokio::test]
async fn a_deadline_lands_while_the_writer_is_behind() {
    const BRIEF_MS: u64 = 300;
    let (run, _root) = started_within(&[YES], Some(BRIEF_MS));
    let pid = run.spawned.pid;
    let (lines, mut untaken) = mpsc::channel::<Bytes>(QUEUED);
    let driving = tokio::spawn(run.drive(9, CancellationToken::new(), lines));
    tokio::time::timeout(PATIENCE, async {
        while untaken.len() < QUEUED {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    tokio::time::timeout(PATIENCE, async {
        while rustix::process::test_kill_process(pid).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    assert!(
        !driving.is_finished(),
        "the drain still waits on the writer"
    );
    let mut last = None;
    while let Some(line) = untaken.recv().await {
        last = Some(serde_json::from_slice::<Value>(&line).unwrap());
    }
    let last = last.unwrap();
    assert_eq!(last["method"], "process/exited");
    assert_eq!(last["params"]["ending"]["kind"], "timed_out");
    assert_eq!(
        tokio::time::timeout(PATIENCE, driving)
            .await
            .unwrap()
            .unwrap(),
        9
    );
}

/// A stop told to a driver whose writer is behind lands at once: the process
/// is killed while the writer still has not read, and the chunk the driver
/// was holding for it is forwarded by the drain, before the ending, not lost.
#[tokio::test]
async fn a_stop_lands_while_the_writer_is_behind() {
    let (run, _root) = started_within(&[YES], None);
    let pid = run.spawned.pid;
    let (lines, mut untaken) = mpsc::channel::<Bytes>(QUEUED);
    let stop = CancellationToken::new();
    let driving = tokio::spawn(run.drive(7, stop.clone(), lines));
    tokio::time::timeout(PATIENCE, async {
        while untaken.len() < QUEUED {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    stop.cancel();

    tokio::time::timeout(PATIENCE, async {
        while rustix::process::test_kill_process(pid).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !driving.is_finished(),
        "the drain still waits on the writer"
    );
    let mut methods = Vec::new();
    while let Some(line) = untaken.recv().await {
        let message: Value = serde_json::from_slice(&line).unwrap();
        methods.push(message["method"].as_str().unwrap().to_owned());
    }
    assert!(methods.len() > QUEUED, "the held chunk reached the writer");
    assert_eq!(methods.last().map(String::as_str), Some("process/exited"));
    assert_eq!(
        tokio::time::timeout(PATIENCE, driving)
            .await
            .unwrap()
            .unwrap(),
        7
    );
}
