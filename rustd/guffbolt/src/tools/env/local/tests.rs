//! What a command `LocalEnv` starts can reach and leave behind: only the
//! variables it is given, a terminal when it asks for one, and no process
//! of its group once it is stopped, timed out or dropped.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bytes::Bytes;
use rustix::process::{Pid, test_kill_process};

use super::LocalEnv;
use super::process::DEFAULT_PATH;
use crate::tools::env::{Ending, Env as _, Process, ProcessEvent, Spawn, Stream};

/// A shell that leaves a job in the background, says the job's number and
/// its own, then waits on a second job in the foreground.
const BACKGROUNDED: &str = "sleep 60 & echo $!; echo $$; sleep 60";
/// [`BACKGROUNDED`], every process of it deaf to the hangup a terminal sends
/// its group when the terminal's session leader dies, so only a kill of the
/// whole group ends the job.
const BACKGROUNDED_DEAF: &str = "trap '' HUP; sleep 60 & echo $!; echo $$; sleep 60";
/// How long a killed process may linger before whoever adopted it reaps it.
const GONE_WITHIN: Duration = Duration::from_secs(5);
/// How often a process that should be gone is looked for.
const LOOK_AGAIN: Duration = Duration::from_millis(20);
/// A variable the spawn names, which the child must see.
const NAMED: (&str, &str) = ("NAMED_X", "given");

fn env() -> LocalEnv {
    LocalEnv::new(std::env::temp_dir()).expect("the temporary directory exists")
}

/// The process numbers `output` names, one per line.
fn numbers(output: &str) -> Vec<Pid> {
    output
        .lines()
        .map(|line| {
            let raw = line.trim().parse().expect("each line is a process number");
            Pid::from_raw(raw).expect("a process number is positive")
        })
        .collect()
}

/// The first `count` lines `process` says, read as they arrive.
async fn first_lines(process: &mut Process, count: usize) -> String {
    let mut said = String::new();
    while said.lines().count() < count || !said.ends_with('\n') {
        match process.events.recv().await {
            Some(ProcessEvent::Output { data, .. }) => {
                said.push_str(&String::from_utf8_lossy(&data));
            }
            other => panic!("the process ended before it said {count} lines: {other:?}"),
        }
    }
    said
}

/// Whether `pid` names no process before [`GONE_WITHIN`] passes.
fn gone(pid: Pid) -> bool {
    let deadline = Instant::now() + GONE_WITHIN;
    while test_kill_process(pid).is_ok() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(LOOK_AGAIN);
    }
    true
}

/// How `spawn` ended, everything it said, and the streams it said it on.
async fn run(spawn: &Spawn) -> (Option<Ending>, String, BTreeSet<&'static str>) {
    let process = env().spawn(spawn).await.expect("the program starts");
    let mut said = Vec::new();
    let mut streams = BTreeSet::new();
    let ending = process
        .ended(|stream, data: Bytes| {
            streams.insert(match stream {
                Stream::Stdout => "stdout",
                Stream::Stderr => "stderr",
                Stream::Terminal => "terminal",
            });
            said.extend_from_slice(&data);
        })
        .await;
    (ending, String::from_utf8_lossy(&said).into_owned(), streams)
}

#[tokio::test]
async fn a_timed_out_command_leaves_no_child_running() {
    let spawn = Spawn::program("sh")
        .args(["-c", BACKGROUNDED])
        .timeout(Duration::from_secs(1));

    let (ending, said, _streams) = run(&spawn).await;

    assert_eq!(ending, Some(Ending::TimedOut));
    let started = numbers(&said);
    assert_eq!(started.len(), 2, "the job and the shell: {said:?}");
    for pid in started {
        assert!(gone(pid), "{pid:?} still runs after the timeout");
    }
}

#[tokio::test]
async fn a_stopped_command_leaves_no_child_running() {
    let env = env();
    let mut process = env
        .spawn(&Spawn::program("sh").args(["-c", BACKGROUNDED]))
        .await
        .expect("sh starts");
    let started = numbers(&first_lines(&mut process, 2).await);

    env.kill(process.id).await.expect("the process is running");
    let ending = process.ended(|_stream, _data| {}).await;

    assert_eq!(ending, Some(Ending::Signaled(9)));
    for pid in started {
        assert!(gone(pid), "{pid:?} still runs after the stop");
    }
}

#[tokio::test]
async fn a_timed_out_terminal_command_leaves_no_child_running() {
    let spawn = Spawn::program("sh")
        .args(["-c", BACKGROUNDED_DEAF])
        .terminal()
        .timeout(Duration::from_secs(1));

    let (ending, said, streams) = run(&spawn).await;

    assert_eq!(ending, Some(Ending::TimedOut));
    assert_eq!(streams, BTreeSet::from(["terminal"]));
    let started = numbers(&said);
    assert_eq!(started.len(), 2, "the job and the shell: {said:?}");
    for pid in started {
        assert!(gone(pid), "{pid:?} still runs after the timeout");
    }
}

/// A runtime that stops drops the task watching a process; the group goes
/// with it, as the leader does with its child handle.
#[test]
fn a_dropped_watcher_leaves_no_child_running() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime builds");
    let started = runtime.block_on(async {
        let env = env();
        let mut process = env
            .spawn(&Spawn::program("sh").args(["-c", BACKGROUNDED]))
            .await
            .expect("sh starts");
        numbers(&first_lines(&mut process, 2).await)
    });

    drop(runtime);

    assert_eq!(started.len(), 2);
    for pid in started {
        assert!(
            gone(pid),
            "{pid:?} still runs after its watcher was dropped"
        );
    }
}

#[tokio::test]
async fn a_local_command_sees_only_named_variables() {
    let inherited: BTreeSet<String> = std::env::vars_os()
        .filter_map(|(name, _value)| name.into_string().ok())
        .filter(|name| name != "PATH")
        .collect();
    assert!(
        !inherited.is_empty(),
        "the test process holds a variable a child could inherit"
    );

    let (ending, said, _streams) = run(&Spawn::program("env").env(NAMED.0, NAMED.1)).await;

    assert_eq!(ending, Some(Ending::Exited(0)));
    let seen: BTreeSet<&str> = said.lines().collect();
    let path = format!("PATH={DEFAULT_PATH}");
    let named = format!("{}={}", NAMED.0, NAMED.1);
    assert_eq!(seen, BTreeSet::from([path.as_str(), named.as_str()]));
    for name in &inherited {
        assert!(
            !said.contains(&format!("{name}=")),
            "{name} reached the child"
        );
    }
}

#[tokio::test]
async fn a_spawn_naming_its_own_search_path_replaces_the_default() {
    let spawn = Spawn::program("/usr/bin/env").env("PATH", "/bin");

    let (ending, said, _streams) = run(&spawn).await;

    assert_eq!(ending, Some(Ending::Exited(0)));
    assert_eq!(said.lines().collect::<Vec<_>>(), ["PATH=/bin"]);
}

#[tokio::test]
async fn a_terminal_request_runs_on_a_terminal() {
    let spawn = Spawn::program("sh")
        .args(["-c", "test -t 0 && test -t 1 && echo on-a-terminal"])
        .terminal();

    let (ending, said, streams) = run(&spawn).await;

    assert_eq!(ending, Some(Ending::Exited(0)));
    assert_eq!(said, "on-a-terminal\r\n");
    assert_eq!(streams, BTreeSet::from(["terminal"]));
}

#[tokio::test]
async fn a_command_on_pipes_has_no_terminal() {
    let spawn = Spawn::program("sh").args(["-c", "test -t 1 || echo on-pipes"]);

    let (ending, said, streams) = run(&spawn).await;

    assert_eq!(ending, Some(Ending::Exited(0)));
    assert_eq!(said, "on-pipes\n");
    assert_eq!(streams, BTreeSet::from(["stdout"]));
}

#[tokio::test]
async fn a_terminal_carries_what_is_written_to_it() {
    let env = env();
    let mut process = env
        .spawn(
            &Spawn::program("sh")
                .args(["-c", "read line; echo \"got $line\""])
                .terminal(),
        )
        .await
        .expect("sh starts");

    env.write(process.id, Bytes::from_static(b"hello\n"))
        .await
        .expect("the process takes input");
    let said = first_lines(&mut process, 2).await;

    assert_eq!(
        said, "hello\r\ngot hello\r\n",
        "the terminal echoes, then sh answers"
    );
}
