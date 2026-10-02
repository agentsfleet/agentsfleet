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
        .spawn(Spawn::program("cat").terminal())
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
    assert_eq!(finished.endings, [(Ending::Signaled(15), 0)]);
}

#[tokio::test]
async fn a_terminal_process_that_exits_reports_its_output_and_status() {
    let harness = start().await;

    let spawn = Spawn::program("sh")
        .args(["-c", "echo ready; exit 4"])
        .terminal();
    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    assert_eq!(
        finished.terminal, b"ready\r\n",
        "a terminal ends lines with a carriage return"
    );
    assert!(finished.stdout.is_empty() && finished.stderr.is_empty());
    assert_eq!(finished.endings, [(Ending::Exited(4), 0)]);
}
