//! The run's `Sessions`: the cap, room made and places freed.

use super::*;

/// Opens `count` hanging sessions on `fake`, ids 1 to `count`.
async fn open_hanging(sessions: &Sessions, fake: &Fake, count: usize) {
    for _ in 0..count {
        let slot = sessions.make_room(fake).await;
        let process = fake
            .spawn(&Spawn::program("hang"))
            .await
            .expect("the fake starts it");
        let _shared = sessions.open(slot, process);
    }
}

/// At the cap, room is made from the least recently used session that
/// already ended, without a kill.
#[tokio::test]
async fn at_the_cap_room_is_made_from_an_ended_session_first() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, SESSIONS_PER_RUN_MAX).await;
    fake.end(5, Ending::Exited(0));

    let _slot = sessions.make_room(&fake).await;

    assert!(
        sessions.get(ProcessId::new(5)).is_none(),
        "the ended one went"
    );
    assert!(
        sessions.get(ProcessId::new(1)).is_some(),
        "the oldest running one stayed"
    );
    assert_eq!(lock(&fake.killed).len(), 0);
}

/// With none ended, the least recently used running session is killed; one
/// a call is reading, and one used since, are passed over.
#[tokio::test]
async fn at_the_cap_the_least_recently_used_running_session_is_killed() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, SESSIONS_PER_RUN_MAX).await;
    let read = sessions.get(ProcessId::new(1)).expect("open");
    let reading = read.lock().await;
    let _used = sessions.get(ProcessId::new(2)).expect("open");

    let _slot = sessions.make_room(&fake).await;

    assert_eq!(*lock(&fake.killed), [3]);
    assert!(sessions.get(ProcessId::new(3)).is_none());
    assert!(
        sessions.get(ProcessId::new(1)).is_some(),
        "the one being read stayed"
    );
    drop(reading);
}

/// The run's end forgets an ended session, kills a running one, and counts
/// a kill the environment refused as killed all the same.
#[tokio::test]
async fn close_all_kills_the_running_sessions_and_forgets_the_ended() {
    let fake = Fake {
        refuse_kill: true,
        ..Fake::default()
    };
    let sessions = Sessions::default();
    open_hanging(&sessions, &fake, 3).await;
    fake.end(2, Ending::Exited(0));

    let running = sessions.close_all(&fake).await;

    assert_eq!(running, 2);
    assert_eq!(*lock(&fake.killed), [1, 3]);
    assert!(sessions.is_quiescent());
    assert_eq!(sessions.close_all(&fake).await, 0, "nothing is left");
}

/// A place held and dropped unopened, as a cancelled call leaves it, is
/// freed; a session closed twice is forgotten once.
#[tokio::test]
async fn a_dropped_place_is_freed_and_a_session_closes_once() {
    let fake = Fake::default();
    let sessions = Sessions::default();

    let slot = sessions.make_room(&fake).await;
    let held = sessions.is_quiescent();
    drop(slot);
    let freed = sessions.is_quiescent();
    open_hanging(&sessions, &fake, 1).await;
    sessions.close(ProcessId::new(1), Ending::Exited(0));
    sessions.close(ProcessId::new(1), Ending::Exited(0));

    assert!(!held);
    assert!(freed);
    assert!(sessions.get(ProcessId::new(1)).is_none());
    assert!(sessions.is_quiescent());
}
