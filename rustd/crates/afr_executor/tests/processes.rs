//! Processes on pipes: output, input, endings, the group kill and the timeout.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use afr_executor::{Ending, Executor as _, Spawn};
use bytes::Bytes;
use rustix::process::{Pid, test_kill_process};

use crate::support::{KIB, finish, read_until, start};

#[tokio::test]
async fn test_executor_spawn_streams_output() {
    let harness = start().await;

    let process = harness
        .client
        .spawn(Spawn::program("echo").arg("hi"))
        .await
        .unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.stdout, b"hi\n");
    assert_eq!(finished.endings, [(Ending::Exited(0), 0)]);
}

#[tokio::test]
async fn standard_error_and_a_failing_status_are_reported_as_they_are() {
    let harness = start().await;

    let spawn = Spawn::program("sh").args(["-c", "echo oops >&2; exit 3"]);
    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    assert_eq!(finished.stderr, b"oops\n");
    assert!(finished.stdout.is_empty());
    assert_eq!(finished.endings, [(Ending::Exited(3), 0)]);
}

#[tokio::test]
async fn a_process_reads_what_is_written_to_it() {
    let harness = start().await;
    let mut process = harness.client.spawn(Spawn::program("cat")).await.unwrap();

    harness
        .client
        .write(process.id, Bytes::from_static(b"hello\n"))
        .await
        .unwrap();
    let echoed = read_until(&mut process, "hello").await;
    harness.client.kill(process.id).await.unwrap();

    assert_eq!(echoed, "hello\n");
    assert_eq!(finish(process).await.endings, [(Ending::Signaled(15), 0)]);
}

#[tokio::test]
async fn test_executor_kill_reaps_process_group() {
    let harness = start().await;
    // The parent ignores TERM and so does the grandchild it leaves behind,
    // which inherits the ignored disposition across its exec.
    let script = "trap '' TERM; sleep 60 & echo $!; wait";
    let mut process = harness
        .client
        .spawn(Spawn::program("sh").args(["-c", script]))
        .await
        .unwrap();
    let printed = read_until(&mut process, "\n").await;
    let grandchild = Pid::from_raw(printed.trim().parse().unwrap()).unwrap();

    harness.client.kill(process.id).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(
        finished.endings,
        [(Ending::Signaled(9), 0)],
        "TERM was ignored, so KILL ended it"
    );
    let mut gone = false;
    for _poll in 0..100 {
        if test_kill_process(grandchild).is_err() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(gone, "the grandchild went with its group");
}

#[tokio::test]
async fn a_process_past_its_timeout_is_stopped_and_says_so() {
    let harness = start().await;

    let spawn = Spawn::program("sleep")
        .arg("30")
        .timeout(Duration::from_millis(100));
    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    assert_eq!(finished.endings, [(Ending::TimedOut, 0)]);
}

#[tokio::test]
async fn output_past_both_edges_is_counted_not_sent() {
    let harness = start().await;

    let spawn = Spawn::program("sh").args(["-c", "yes | head -c 3000000"]);
    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    let kept = finished.stdout.len() as u64;
    let [(Ending::Exited(0), omitted)] = finished.endings.as_slice() else {
        panic!("one clean exit, got {:?}", finished.endings);
    };
    assert_eq!(kept + omitted, 3_000_000);
    assert_eq!(kept, u64::try_from(2 * 512 * KIB).unwrap());
}

#[tokio::test]
async fn a_process_starts_in_the_directory_it_names_and_sees_only_its_environment() {
    let harness = start().await;
    std::fs::create_dir(harness.root.join("sub")).unwrap();

    let spawn = Spawn::program("sh")
        .args(["-c", "pwd; echo $GREETING; echo ${HOME:-unset}"])
        .cwd("sub")
        .env("GREETING", "hej");
    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    let text = String::from_utf8(finished.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.first().unwrap().ends_with("/workspace/sub"), "{text}");
    assert_eq!(lines.get(1..), Some(&["hej", "unset"][..]));
}

#[tokio::test]
async fn a_program_that_does_not_exist_is_refused_at_spawn() {
    let harness = start().await;

    let refused = harness
        .client
        .spawn(Spawn::program("no-such-program-anywhere"))
        .await
        .unwrap_err();

    assert!(
        !refused.is_path_refused() && !refused.is_connection_lost(),
        "{refused}"
    );
}

#[tokio::test]
async fn a_working_directory_outside_the_workspace_is_refused() {
    let harness = start().await;

    let refused = harness
        .client
        .spawn(Spawn::program("pwd").cwd("../"))
        .await
        .unwrap_err();

    assert!(refused.is_path_refused(), "{refused}");
}

#[tokio::test]
async fn a_process_that_already_ended_is_unknown_to_kill_and_write() {
    let harness = start().await;
    let process = harness.client.spawn(Spawn::program("true")).await.unwrap();
    let id = process.id;
    finish(process).await;

    // The session forgets a process once its task is collected; give it a turn.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let killed = harness.client.kill(id).await.unwrap_err();
    let written = harness
        .client
        .write(id, Bytes::from_static(b"x"))
        .await
        .unwrap_err();

    assert!(killed.is_unknown_process(), "{killed}");
    assert!(written.is_unknown_process(), "{written}");
}
