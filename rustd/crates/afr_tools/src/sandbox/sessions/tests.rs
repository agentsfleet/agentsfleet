#![expect(
    clippy::unwrap_used,
    reason = "test module: a scripted spawn with a script left cannot fail"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afr_executor::{Ending, Executor, ProcessId, Spawn};

use super::{
    EVENT_COMPLETED, EVENT_KILL_REFUSED, EVENT_STARTED, SESSIONS_PER_LEASE_MAX, SESSIONS_PROTECTED,
    Sessions,
};
use crate::sandbox::{ScriptedExecutor, ScriptedProcess};

/// How long making room may take when nothing it chooses is held.
const ROOM_MADE_WITHIN: Duration = Duration::from_secs(5);
/// The program every scripted session names.
const SHELL: &str = "/bin/sh";
/// The scripts [`full`] leaves for sessions a test starts itself.
const STARTS_LEFT: usize = 2;

/// Starts the next scripted process and keeps it as a session.
async fn opened(executor: &ScriptedExecutor, sessions: &mut Sessions) -> ProcessId {
    let slot = sessions.make_room(executor).await;
    let process = executor.spawn(&Spawn::program(SHELL)).await.unwrap();
    let id = process.id;
    sessions.open(slot, process);
    id
}

/// How each session that left the registry ended, in the order they left.
fn endings(capture: &Capture) -> Vec<String> {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(EVENT_COMPLETED))
        .filter_map(|event| event.field("ending").map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn should_kill_every_running_session_and_forget_an_ended_one() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([
        ScriptedProcess::stays_open(""),
        ScriptedProcess::stays_open(""),
        ScriptedProcess::stays_open(""),
    ]);
    let mut sessions = Sessions::default();
    let running = opened(&executor, &mut sessions).await;
    let also_running = opened(&executor, &mut sessions).await;
    let exited = opened(&executor, &mut sessions).await;
    assert!(executor.end(exited, Ending::Exited(0)));

    let killed = sessions.close_all(&executor).await;

    assert_eq!(killed, 2);
    assert_eq!(executor.killed(), [running, also_running]);
    for id in [running, also_running, exited] {
        assert!(sessions.get(id).is_none(), "{id:?} is gone");
    }
    assert_eq!(endings(&capture), ["interrupted", "interrupted", "exited"]);
}

#[tokio::test]
async fn should_forget_a_session_whose_kill_is_refused() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut sessions = Sessions::default();
    let id = opened(&executor, &mut sessions).await;
    assert!(executor.forget(id));

    let killed = sessions.close_all(&executor).await;

    assert_eq!(killed, 1);
    assert!(sessions.get(id).is_none());
    let refused = capture.only(EVENT_KILL_REFUSED);
    assert_eq!(refused.field("session_id"), Some("1"));
}

#[tokio::test]
async fn should_not_kill_a_session_whose_executor_is_gone() {
    let executor = ScriptedExecutor::new([ScriptedProcess::vanishes("")]);
    let mut sessions = Sessions::default();
    opened(&executor, &mut sessions).await;

    let killed = sessions.close_all(&executor).await;

    assert_eq!(killed, 0);
    let killed = executor.killed();
    assert!(killed.is_empty(), "{killed:?}");
}

#[tokio::test]
async fn should_log_a_session_opening_and_closing_with_its_ending() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut sessions = Sessions::default();
    let id = opened(&executor, &mut sessions).await;

    sessions.close(id, Ending::Exited(3));

    assert!(sessions.get(id).is_none());
    assert_eq!(capture.only(EVENT_STARTED).field("session_id"), Some("1"));
    assert_eq!(endings(&capture), ["exited"]);
}

/// `count` running sessions, oldest first.
async fn full(count: usize) -> (ScriptedExecutor, Sessions, Vec<ProcessId>) {
    let executor =
        ScriptedExecutor::new((0..count + STARTS_LEFT).map(|_| ScriptedProcess::stays_open("")));
    let mut sessions = Sessions::default();
    let mut ids = Vec::new();
    for _ in 0..count {
        ids.push(opened(&executor, &mut sessions).await);
    }
    (executor, sessions, ids)
}

/// The sessions still open, by id.
fn still_open(sessions: &mut Sessions, ids: &[ProcessId]) -> usize {
    ids.iter().filter(|id| sessions.get(**id).is_some()).count()
}

#[tokio::test]
async fn should_leave_every_session_open_under_the_cap() {
    let (executor, mut sessions, ids) = full(SESSIONS_PER_LEASE_MAX - 1).await;

    sessions.make_room(&executor).await;

    assert_eq!(still_open(&mut sessions, &ids), SESSIONS_PER_LEASE_MAX - 1);
    let killed = executor.killed();
    assert!(killed.is_empty(), "{killed:?}");
}

/// At the cap, Codex's choice: the least recently used session that already
/// ended goes first, ahead of an older one still running, and is not killed.
#[tokio::test]
async fn should_make_room_by_forgetting_the_oldest_ended_session() {
    let (executor, mut sessions, ids) = full(SESSIONS_PER_LEASE_MAX).await;
    let ended = *ids.get(2).unwrap();
    assert!(executor.end(ended, Ending::Exited(0)));

    sessions.make_room(&executor).await;

    assert!(sessions.get(ended).is_none());
    assert_eq!(still_open(&mut sessions, &ids), SESSIONS_PER_LEASE_MAX - 1);
    assert!(
        executor.killed().is_empty(),
        "an ended session is not killed"
    );
}

/// With none ended, the least recently used running session is killed; a
/// session read since it opened counts as used then.
#[tokio::test]
async fn should_make_room_by_killing_the_least_recently_used_session() {
    let capture = Capture::install();
    let (executor, sessions, ids) = full(SESSIONS_PER_LEASE_MAX).await;
    let (first, second) = (*ids.first().unwrap(), *ids.get(1).unwrap());
    sessions.get(first).unwrap();

    sessions.make_room(&executor).await;

    assert_eq!(executor.killed(), [second]);
    assert!(sessions.get(first).is_some());
    assert_eq!(endings(&capture), ["interrupted"]);
}

/// The most recently used sessions are never the ones pruned, even when one
/// of them already ended and every older one still runs.
#[tokio::test]
async fn should_never_prune_the_most_recently_used_sessions() {
    let (executor, sessions, ids) = full(SESSIONS_PER_LEASE_MAX).await;
    let recent = *ids
        .get(SESSIONS_PER_LEASE_MAX - SESSIONS_PROTECTED)
        .unwrap();
    assert!(executor.end(recent, Ending::Exited(0)));

    sessions.make_room(&executor).await;

    assert!(sessions.get(recent).is_some(), "a recent session stays");
    assert_eq!(executor.killed(), [*ids.first().unwrap()]);
}

/// A session a call is reading is in use, so room is made from the next
/// least recently used one, even though the held one is older.
#[tokio::test]
async fn should_never_prune_a_session_a_call_holds() {
    let (executor, sessions, ids) = full(SESSIONS_PER_LEASE_MAX).await;
    let (first, second) = (*ids.first().unwrap(), *ids.get(1).unwrap());
    let held = sessions.get(first).unwrap();
    let reading = held.lock().await;
    // Every other session read since, so `first` is again the least
    // recently used, and `second` the next.
    for id in ids.iter().skip(1) {
        sessions.get(*id).unwrap();
    }

    // A held session chosen would wait on its reader forever.
    let made = tokio::time::timeout(ROOM_MADE_WITHIN, sessions.make_room(&executor)).await;
    assert!(
        made.is_ok(),
        "room is made without waiting on the session a call holds"
    );

    assert!(sessions.get(first).is_some(), "the held session stays");
    assert_eq!(executor.killed(), [second]);
    drop(reading);
}

/// Two calls closing one session, or a call closing one room was made from,
/// leave one completion behind.
#[tokio::test]
async fn should_log_a_session_ending_once_when_it_is_closed_twice() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut sessions = Sessions::default();
    let id = opened(&executor, &mut sessions).await;

    sessions.close(id, Ending::Exited(0));
    sessions.close(id, Ending::Exited(0));

    assert_eq!(endings(&capture), ["exited"]);
}

/// Two calls starting sessions at once one short of the cap both hold their
/// place before either process starts, so the second makes room.
#[tokio::test]
async fn should_hold_the_cap_when_two_sessions_start_at_once() {
    let (executor, sessions, ids) = full(SESSIONS_PER_LEASE_MAX - 1).await;

    let slots = [
        sessions.make_room(&executor).await,
        sessions.make_room(&executor).await,
    ];
    for slot in slots {
        sessions.open(slot, executor.spawn(&Spawn::program(SHELL)).await.unwrap());
    }

    assert_eq!(sessions.book().open.len(), SESSIONS_PER_LEASE_MAX);
    assert_eq!(executor.killed(), [*ids.first().unwrap()]);
}

/// A place dropped unopened, as a failed spawn or a cancelled call drops it,
/// is freed: the next start one short of the cap makes no room.
#[tokio::test]
async fn should_free_a_place_dropped_unopened() {
    let (executor, sessions, _ids) = full(SESSIONS_PER_LEASE_MAX - 1).await;

    drop(sessions.make_room(&executor).await);
    let _slot = sessions.make_room(&executor).await;

    let killed = executor.killed();
    assert!(killed.is_empty(), "{killed:?}");
}
