//! Processes on a pseudo-terminal.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afr_executor::{Ending, Executor as _, Spawn};
use bytes::Bytes;

use crate::support::{finish, read_until, start};

#[tokio::test]
async fn test_executor_pty_accepts_input() {
    let harness = start().await;
    let mut process = harness
        .client
        .spawn(&Spawn::program("cat").terminal())
        .await
        .unwrap();

    harness
        .client
        .write(process.id, Bytes::from_static(b"x"))
        .await
        .unwrap();
    let echoed = read_until(&mut process, "x").await;
    harness.client.kill(process.id).await.unwrap();

    assert_eq!(echoed, "x", "the terminal echoes what was typed");
    let finished = finish(process).await;
    assert_eq!(finished.endings, [Ending::Signaled(15)]);
}

#[tokio::test]
async fn a_terminal_process_that_exits_reports_its_output_and_status() {
    let harness = start().await;

    let spawn = Spawn::program("sh")
        .args(["-c", "echo ready; exit 4"])
        .terminal();
    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    assert_eq!(
        finished.terminal, b"ready\r\n",
        "a terminal ends lines with a carriage return"
    );
    assert!(finished.stdout.is_empty() && finished.stderr.is_empty());
    assert_eq!(finished.endings, [Ending::Exited(4)]);
    assert!(!finished.abandoned, "its terminal closed with it");
}

/// What `stty size` prints for the terminal the executor opens: rows, then
/// columns.
const GEOMETRY: &str = "40 160";

/// A process on a terminal leads a session of its own, which that terminal
/// controls: `/dev/tty` opens, and reads back the size the executor set.
#[tokio::test]
async fn a_terminal_process_leads_a_session_its_sized_terminal_controls() {
    let harness = start().await;

    let spawn = Spawn::program("sh")
        .args(["-c", "tty; stty size < /dev/tty"])
        .terminal();
    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    let said = String::from_utf8_lossy(&finished.terminal);
    let lines: Vec<&str> = said.lines().map(str::trim_end).collect();
    assert!(
        lines.first().is_some_and(|name| name.starts_with("/dev/")),
        "its input is a terminal: {said:?}"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(GEOMETRY),
        "its controlling terminal is the one sized for it: {said:?}"
    );
    assert_eq!(finished.endings, [Ending::Exited(0)]);
}
