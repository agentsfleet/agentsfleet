#![expect(
    clippy::unwrap_used,
    reason = "test module: a scripted spawn with a script left cannot fail"
)]

use afd_core::test_util::trace::Capture;
use afr_executor::{Ending, Executor, ProcessId, Spawn};

use super::{EVENT_COMPLETED, EVENT_KILL_REFUSED, EVENT_STARTED, SESSIONS_PER_LEASE_MAX, Sessions};
use crate::sandbox::{ScriptedExecutor, ScriptedProcess};

/// Starts the next scripted process and keeps it as a session.
async fn opened(executor: &ScriptedExecutor, sessions: &mut Sessions) -> ProcessId {
    let process = executor.spawn(&Spawn::program("/bin/sh")).await.unwrap();
    sessions.open(process).id
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
        assert!(sessions.get_mut(id).is_none(), "{id:?} is gone");
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
    assert!(sessions.get_mut(id).is_none());
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
    assert!(executor.killed().is_empty());
}

#[tokio::test]
async fn should_log_a_session_opening_and_closing_with_its_ending() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("")]);
    let mut sessions = Sessions::default();
    let id = opened(&executor, &mut sessions).await;

    sessions.close(id, Ending::Exited(3));

    assert!(sessions.get_mut(id).is_none());
    assert_eq!(capture.only(EVENT_STARTED).field("session_id"), Some("1"));
    assert_eq!(endings(&capture), ["exited"]);
}

#[tokio::test]
async fn should_have_room_for_exactly_the_cap() {
    let executor =
        ScriptedExecutor::new((0..SESSIONS_PER_LEASE_MAX).map(|_| ScriptedProcess::stays_open("")));
    let mut sessions = Sessions::default();

    for _ in 0..SESSIONS_PER_LEASE_MAX {
        assert!(sessions.has_room());
        opened(&executor, &mut sessions).await;
    }

    assert!(!sessions.has_room());
}
