//! Once the drain gives up on output a process's child keeps writing, the
//! output is no longer read: the reader lets go, and the writer finds no one
//! reading. A kill reaches the group once after its leader exited, and never
//! a process that left it.

use super::*;

/// A command that leaves a child writing a line every 50 ms, writing that
/// child's number to `child`, then exits at once.
#[cfg(unix)]
const LEAVES_A_WRITER: &str = "(while :; do echo more; sleep 0.05; done) & echo $! > child; exit 0";

/// A command that leaves a child in a session of its own holding the output,
/// outside the command's group, writing its number to `child`, then exits.
#[cfg(unix)]
const LEAVES_THE_GROUP: &str =
    "perl -MPOSIX -e 'POSIX::setsid(); sleep 30' & echo $! > child; exit 0";

/// Runs [`LEAVES_A_WRITER`] in `env` and answers its ending once the drain
/// gave up, and whether the writer was gone soon after; one left running is
/// killed.
#[cfg(unix)]
async fn abandoned(directory: &tempfile::TempDir, env: &LocalEnv) -> (Option<Ending>, bool) {
    let spawn = Spawn::program("/bin/sh").arg("-c").arg(LEAVES_A_WRITER);
    let process = env.spawn(&spawn).await.expect("started");
    let child = written_child(&directory.path().join("child")).await;
    let ended = process.ended(|_, _| {}).await;
    (ended, gone(child).await)
}

#[cfg(unix)]
#[tokio::test]
async fn output_on_pipes_after_the_drain_gave_up_is_dropped() {
    let (directory, env) = local();

    let ended = abandoned(&directory, &env).await;

    assert_eq!(ended, (Some(Ending::Exited(0)), true));
}

/// After the leader exited, the first kill signals its group and the second
/// signals nothing: the group's number may be another's by then. A process
/// that left the group in a session of its own is beyond both, and the
/// drain gives up on the output it holds.
#[cfg(unix)]
#[tokio::test]
async fn a_kill_after_the_leader_exited_signals_its_group_once() {
    let (directory, env) = local();
    let spawn = Spawn::program("/bin/sh").arg("-c").arg(LEAVES_THE_GROUP);
    let process = env.spawn(&spawn).await.expect("started");
    let child = written_child(&directory.path().join("child")).await;
    // The child leads a group of its own once it has left the command's.
    for _ in 0..200 {
        if rustix::process::getpgid(Some(child)).is_ok_and(|group| group == child) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    env.kill(process.id)
        .await
        .expect("held while its output drains");
    env.kill(process.id).await.expect("still held");
    let ended = process.ended(|_, _| {}).await;
    let escaped = rustix::process::test_kill_process(child).is_ok();
    let _killed = rustix::process::kill_process(child, rustix::process::Signal::KILL);

    assert_eq!(ended, Some(Ending::Signaled(9)));
    assert!(
        escaped,
        "a process outside the group is not the group's to kill"
    );
}
