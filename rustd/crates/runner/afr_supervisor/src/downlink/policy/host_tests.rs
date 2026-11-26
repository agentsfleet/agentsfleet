//! Every beat reports what the host offers its runs, and a fact the runner
//! cannot read is absent from the beat and logged.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use af_core::test_util::trace::Capture;

use super::Heartbeat;
use super::tests::{KEEP_GOING, idle_holds, probe, reply, sent};
use crate::test_support::{drain, plane};

/// A beat reports the processors the runner may use, the free bytes of the
/// file system that holds its sandboxes, and the host's memory where the host
/// answers it; a sandbox root it cannot read reports no free disk, and says so
/// once while it stays unread.
#[tokio::test]
async fn a_beat_carries_the_hosts_processors_memory_and_free_disk() {
    let (plane, mut calls) = plane(|_call| reply(KEEP_GOING, true));
    let probe = probe();
    let held = idle_holds();
    let sandboxes = tempfile::tempdir().unwrap();
    let missing = sandboxes.path().join("missing");

    let read = Heartbeat::new(&plane, &probe, &held, sandboxes.path())
        .beat()
        .await;
    let log = Capture::install();
    let mut unreadable = Heartbeat::new(&plane, &probe, &held, &missing);
    let unread = unreadable.beat().await;
    let again = unreadable.beat().await;
    let unread_line = log.only("runner_host_fact_unread");
    drop(log);

    assert!(
        read.is_ok() && unread.is_ok() && again.is_ok(),
        "{read:?} {unread:?} {again:?}"
    );
    let calls = drain(&mut calls);
    let (read, unread) = (&sent(&calls[0])["host"], &sent(&calls[1])["host"]);
    assert!(
        read["cpu_count"].as_u64().is_some_and(|cpus| cpus >= 1),
        "{read}"
    );
    assert!(
        read["free_disk_bytes"]
            .as_u64()
            .is_some_and(|free| free > 0),
        "{read}"
    );
    assert_eq!(
        read["memory_bytes"].as_u64().is_some(),
        cfg!(target_os = "linux"),
        "{read}"
    );
    assert!(unread.get("free_disk_bytes").is_none(), "{unread}");
    assert_eq!(
        (unread_line.field("fact"), unread_line.field("error_code")),
        (
            Some("free_disk_bytes"),
            Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
        )
    );
}
