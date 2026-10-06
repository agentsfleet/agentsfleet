//! Processes on pipes: output, input, endings, the group kill and the timeout.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use afr_executor::{Ending, Executor as _, Spawn};
use bytes::Bytes;
use rustix::process::{Pid, test_kill_process};

use crate::support::{
    INVALID_PARAMS, KIB, PATH_REFUSED, PATIENCE, UNKNOWN_PROCESS, finish, read_until, refused_with,
    start,
};

#[tokio::test]
async fn test_executor_spawn_streams_output() {
    let harness = start().await;

    let process = harness
        .client
        .spawn(&Spawn::program("echo").arg("hi"))
        .await
        .unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.stdout, b"hi\n");
    assert_eq!(finished.endings, [Ending::Exited(0)]);
}

#[tokio::test]
async fn standard_error_and_a_failing_status_are_reported_as_they_are() {
    let harness = start().await;

    let spawn = Spawn::program("sh").args(["-c", "echo oops >&2; exit 3"]);
    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    assert_eq!(finished.stderr, b"oops\n");
    assert!(finished.stdout.is_empty());
    assert_eq!(finished.endings, [Ending::Exited(3)]);
}

#[tokio::test]
async fn a_process_reads_what_is_written_to_it() {
    let harness = start().await;
    let mut process = harness.client.spawn(&Spawn::program("cat")).await.unwrap();

    harness
        .client
        .write(process.id, Bytes::from_static(b"hello\n"))
        .await
        .unwrap();
    let echoed = read_until(&mut process, "hello").await;
    harness.client.kill(process.id).await.unwrap();

    assert_eq!(echoed, "hello\n");
    assert_eq!(finish(process).await.endings, [Ending::Signaled(15)]);
}

#[tokio::test]
async fn test_executor_kill_reaps_process_group() {
    let harness = start().await;
    // The parent ignores TERM and so does the grandchild it leaves behind,
    // which inherits the ignored disposition across its exec.
    let script = "trap '' TERM; sleep 60 & echo $!; wait";
    let mut process = harness
        .client
        .spawn(&Spawn::program("sh").args(["-c", script]))
        .await
        .unwrap();
    let printed = read_until(&mut process, "\n").await;
    let grandchild = Pid::from_raw(printed.trim().parse().unwrap()).unwrap();

    harness.client.kill(process.id).await.unwrap();
    let finished = finish(process).await;

    assert_eq!(
        finished.endings,
        [Ending::Signaled(9)],
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
    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    assert_eq!(finished.endings, [Ending::TimedOut]);
}

/// A process that has said more than one edge holds goes on being heard: what
/// it says next arrives while it still runs, not only at its end.
#[tokio::test]
async fn test_a_process_is_heard_past_its_first_half_mebibyte() {
    let harness = start().await;
    let spawn = Spawn::program("sh").args([
        "-c",
        "head -c 700000 /dev/zero; read word; echo said-$word; sleep 30",
    ]);
    let mut process = harness.client.spawn(&spawn).await.unwrap();

    harness
        .client
        .write(process.id, Bytes::from_static(b"go\n"))
        .await
        .unwrap();

    assert!(
        read_until(&mut process, "said-go")
            .await
            .ends_with("said-go\n")
    );
}

/// A caller that reads only once the process has ended holds its first and
/// last half mebibyte and a count of the rest, never all it said.
#[tokio::test]
async fn a_caller_that_reads_only_at_the_end_keeps_the_edges_and_counts_the_rest() {
    let harness = start().await;
    let spawn = Spawn::program("sh").args(["-c", "yes | head -c 3000000"]);
    let process = harness.client.spawn(&spawn).await.unwrap();

    tokio::time::timeout(PATIENCE, process.events.finished())
        .await
        .unwrap();
    let finished = finish(process).await;

    assert_eq!(finished.endings, [Ending::Exited(0)]);
    assert_eq!(finished.stdout.len(), 2 * 512 * KIB);
    assert_eq!(finished.stdout.len() as u64 + finished.omitted, 3_000_000);
}

#[tokio::test]
async fn a_process_starts_in_the_directory_it_names_and_sees_only_its_environment() {
    let harness = start().await;
    std::fs::create_dir(harness.root.join("sub")).unwrap();

    let spawn = Spawn::program("sh")
        .args(["-c", "pwd; echo $GREETING; echo ${HOME:-unset}"])
        .cwd("sub")
        .env("GREETING", "hej");
    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    let text = String::from_utf8(finished.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.first().unwrap().ends_with("/workspace/sub"), "{text}");
    assert_eq!(lines.get(1..), Some(&["hej", "unset"][..]));
}

#[tokio::test]
async fn a_program_that_does_not_exist_is_the_callers_mistake_on_pipes_and_terminals() {
    let harness = start().await;

    let on_pipes = Spawn::program("no-such-program-anywhere");
    let on_terminal = Spawn::program("no-such-program-anywhere").terminal();
    for spawn in [on_pipes, on_terminal] {
        let refused = harness.client.spawn(&spawn).await.unwrap_err();

        assert!(refused_with(&refused, INVALID_PARAMS), "{refused}");
    }
}

#[tokio::test]
async fn a_working_directory_outside_the_workspace_is_refused() {
    let harness = start().await;

    let refused = harness
        .client
        .spawn(&Spawn::program("pwd").cwd("../"))
        .await
        .unwrap_err();

    assert!(refused_with(&refused, PATH_REFUSED), "{refused}");
}

#[tokio::test]
async fn a_process_that_already_ended_is_unknown_to_kill_and_write() {
    let harness = start().await;
    let process = harness.client.spawn(&Spawn::program("true")).await.unwrap();
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

    assert!(refused_with(&killed, UNKNOWN_PROCESS), "{killed}");
    assert!(refused_with(&written, UNKNOWN_PROCESS), "{written}");
}

/// One connection carries a hundred processes speaking at once: each caller
/// hears exactly its own output, all of it, and exactly one ending. An
/// output line routed to the wrong process, or one that arrived before its
/// spawn's answer, shows here as a short or foreign transcript.
#[tokio::test]
async fn test_a_hundred_processes_on_one_connection_each_hear_only_their_own_output() {
    const PROCESSES: usize = 100;
    const SAID: usize = 100_000;
    let harness = start().await;
    let client = &harness.client;

    let finished = futures_util::future::join_all((0..PROCESSES).map(|process| async move {
        let script = format!("yes p{process:03} | head -c {SAID}");
        let spawn = Spawn::program("sh").args(["-c", script.as_str()]);
        (process, finish(client.spawn(&spawn).await.unwrap()).await)
    }))
    .await;

    for (process, finished) in finished {
        let expected: Vec<u8> = format!("p{process:03}\n")
            .into_bytes()
            .into_iter()
            .cycle()
            .take(SAID)
            .collect();
        assert_eq!(finished.endings, [Ending::Exited(0)], "process {process}");
        assert_eq!(finished.omitted, 0, "process {process}");
        assert!(
            finished.stdout == expected,
            "process {process} heard another's output"
        );
    }
}
