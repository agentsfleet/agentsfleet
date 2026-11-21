//! A run's children, read through the book: the caps, a close that frees a
//! seat and closes what the child started, an interrupt that keeps the child
//! open, a pause that holds every start, and a census that comes back whole.

use std::sync::atomic::Ordering;

use super::{Agents, Caps, Census, Counted, Kind, Me, Refused, Status};
use crate::drive::DriveState;
use crate::transcript::{Message, Replay};

/// The caps the cases run under: depth 2, two open.
const CAPS: Caps = Caps { depth: 2, open: 2 };

/// Reserves a child of `parent` over `message`, holding no tool.
fn reserve(agents: &Agents, parent: Me, message: &str) -> Result<u64, Refused> {
    agents.reserve(parent, Vec::new(), Vec::new(), message.to_owned())
}

/// Ends child `id`'s turn with `answer`, keeping `transcript`.
fn answer(agents: &Agents, id: u64, answer: &str, transcript: Vec<Message>) {
    let begun = agents.begin(id);
    assert!(begun.is_some(), "child {id} had a turn queued");
    let status = Status::Done(answer.to_owned());
    agents.ended(id, status, Some((transcript, DriveState::default())));
}

fn kinds(agents: &Agents) -> Vec<Kind> {
    agents.list().iter().map(|child| child.status).collect()
}

mod edges;

#[test]
fn the_caps_refuse_a_child_past_the_open_count() {
    let (agents, _requests) = Agents::new(CAPS);

    assert_eq!(reserve(&agents, Me::ROOT, "a"), Ok(1));
    assert_eq!(reserve(&agents, Me::ROOT, "b"), Ok(2));
    assert_eq!(
        reserve(&agents, Me::ROOT, "c"),
        Err(Refused::Capped),
        "two are open"
    );
    answer(&agents, 1, "a done", Vec::new());
    assert_eq!(
        reserve(&agents, Me::ROOT, "c"),
        Err(Refused::Capped),
        "an idle child keeps its seat"
    );
    assert_eq!(agents.close(1), Some(Kind::Done));
    assert_eq!(reserve(&agents, Me::ROOT, "c"), Ok(3));
    answer(&agents, 2, "b done", Vec::new());
    assert_eq!(agents.close(2), Some(Kind::Done));
    assert_eq!(
        reserve(&agents, Me::ROOT, "d"),
        Ok(4),
        "a fourth start is no count past any cap"
    );
}

/// Six open; a seventh is refused.
#[test]
fn a_seventh_open_subagent_is_refused() {
    let (agents, _requests) = Agents::new(Caps { depth: 2, open: 6 });

    for id in 1..=6 {
        assert_eq!(reserve(&agents, Me::ROOT, "open"), Ok(id));
    }

    assert_eq!(reserve(&agents, Me::ROOT, "seventh"), Err(Refused::Capped));
}

/// No count of starts caps a run: each child closed frees its seat.
#[test]
fn a_run_starts_more_than_sixteen_subagents() {
    let (agents, _requests) = Agents::new(CAPS);

    for id in 1..=20 {
        assert_eq!(reserve(&agents, Me::ROOT, "one more"), Ok(id));
        answer(&agents, id, "done", Vec::new());
        assert_eq!(agents.close(id), Some(Kind::Done));
    }
}

#[test]
fn closing_a_child_closes_every_child_it_started() {
    let (agents, _requests) = Agents::new(Caps { open: 3, ..CAPS });
    let parent = reserve(&agents, Me::ROOT, "lead").ok();
    answer(&agents, 1, "led", Vec::new());
    let grandchild = reserve(&agents, Me { id: 1, depth: 1 }, "follow");

    assert_eq!((parent, grandchild), (Some(1), Ok(2)));
    assert_eq!(agents.close(1), Some(Kind::Done));
    assert_eq!(
        kinds(&agents),
        [Kind::Closed, Kind::Running],
        "the running grandchild closes as its turn ends"
    );
    assert!(agents.begin(2).is_none(), "its turn never starts");
    assert_eq!(kinds(&agents), [Kind::Closed, Kind::Closed]);
    assert_eq!(
        reserve(&agents, Me::ROOT, "next"),
        Ok(3),
        "both seats are free"
    );
}

#[test]
fn an_interrupted_turn_stops_and_the_child_stays_open() {
    let (agents, _requests) = Agents::new(CAPS);
    let id = reserve(&agents, Me::ROOT, "work").ok();
    let turn = agents.begin(1).map(|turn| turn.stop);

    assert_eq!(id, Some(1));
    assert_eq!(agents.interrupt(1), Some(Kind::Running));
    assert!(turn.is_some_and(|stop| stop.is_cancelled()));
    agents.ended(1, Status::Interrupted, None);
    assert_eq!(kinds(&agents), [Kind::Interrupted]);
    assert_eq!(agents.follow_up(1, "again".to_owned()), Some(true));
    assert_eq!(kinds(&agents), [Kind::Running], "it takes another task");
    assert_eq!(agents.interrupt(9), None);
    assert_eq!(agents.close(9), None);
    assert_eq!(agents.send(9, "x".to_owned()), None);
}

#[test]
fn a_pause_refuses_every_start_and_follow_up_until_it_is_dropped() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "a"), Ok(1));
    answer(&agents, 1, "a done", Vec::new());

    let pause = agents.try_pause(|| true);

    assert!(pause.is_some(), "no turn runs");
    assert!(agents.try_pause(|| true).is_none(), "one pause at a time");
    assert_eq!(reserve(&agents, Me::ROOT, "b"), Err(Refused::Capped));
    assert_eq!(agents.follow_up(1, "again".to_owned()), Some(false));
    drop(pause);
    assert!(!agents.0.gate.load(Ordering::Acquire));
    assert_eq!(agents.follow_up(1, "again".to_owned()), Some(true));
}

#[test]
fn a_census_comes_back_whole_and_reads_only_under_its_own_pause() {
    let (agents, _requests) = Agents::new(CAPS);
    let (other, _other_requests) = Agents::new(CAPS);
    let transcript = vec![
        Message::User("count".to_owned()),
        Message::Assistant {
            text: "one".to_owned(),
            calls: Vec::new(),
            replay: Replay::default(),
        },
    ];
    assert_eq!(reserve(&agents, Me::ROOT, "count"), Ok(1));
    answer(&agents, 1, "one", transcript.clone());
    assert_eq!(reserve(&agents, Me::ROOT, "spare"), Ok(2));
    answer(&agents, 2, "spare", Vec::new());
    assert_eq!(agents.close(2), Some(Kind::Done));

    let foreign = other.try_pause(|| true);
    let pause = agents.try_pause(|| true);
    let census = pause.as_ref().and_then(|pause| agents.census(pause));
    let mut state = DriveState::default();
    let saved = pause
        .as_ref()
        .is_some_and(|pause| agents.save(pause, &mut state));

    assert!(
        foreign
            .as_ref()
            .is_some_and(|foreign| agents.census(foreign).is_none())
    );
    assert!(saved);
    let expected = Census {
        next: 2,
        started: 2,
        children: vec![
            Counted {
                id: 1,
                parent: 0,
                depth: 1,
                calls: 0,
                tools: Vec::new(),
                steps: 0,
                status: Status::Done("one".to_owned()),
                transcript,
                state: DriveState::default(),
            },
            Counted {
                id: 2,
                parent: 0,
                depth: 1,
                calls: 0,
                tools: Vec::new(),
                steps: 0,
                status: Status::Closed,
                transcript: Vec::new(),
                state: DriveState::default(),
            },
        ],
    };
    assert_eq!(census.as_ref(), Some(&expected));
    assert_eq!(state.children(), &expected);
    let (restored, _requests) = Agents::restored(CAPS, &state);
    assert_eq!(kinds(&restored), [Kind::Done, Kind::Closed]);
    assert_eq!(
        reserve(&restored, Me::ROOT, "next"),
        Ok(3),
        "ids and counts go on, and the closed child's seat is free"
    );
}

/// A turn begins once and ends once: a running turn is not begun again, an
/// idle child has no turn to begin, and an end recorded twice keeps the
/// first. An id no child has changes nothing.
#[test]
fn a_turn_begins_once_and_ends_once() {
    let (agents, _requests) = Agents::new(CAPS);
    assert_eq!(reserve(&agents, Me::ROOT, "work"), Ok(1));

    assert!(agents.begin(1).is_some(), "the queued turn begins");
    assert!(agents.begin(1).is_none(), "and only once");
    agents.ended(1, Status::Done("one".to_owned()), None);
    agents.ended(1, Status::Interrupted, None);
    assert!(
        agents.begin(1).is_none(),
        "an idle child has no turn queued"
    );
    assert_eq!(kinds(&agents), [Kind::Done], "the first end stands");
    assert_eq!(
        agents.interrupt(1),
        Some(Kind::Done),
        "an idle child has no turn to stop"
    );
    agents.ended(9, Status::Interrupted, None);
    agents.called(9);
    assert_eq!(
        agents.list().iter().map(|child| child.calls).sum::<u64>(),
        0
    );
    assert_eq!(kinds(&agents), [Kind::Done]);
}

/// Ending the set answers an ask for a pause it never read with none.
#[tokio::test]
async fn ending_the_set_answers_an_unread_ask_for_a_pause_with_none() {
    let (agents, requests) = Agents::new(CAPS);
    let mut children = super::children::Children::new(requests);
    let asking = agents.clone();
    let granted = tokio::spawn(async move { asking.pause().await.is_some() });
    // The test's one thread runs the asker as this task yields: it sends its
    // ask and waits on the answer.
    tokio::task::yield_now().await;

    let pause = children.end_all(&agents);

    assert_eq!(granted.await.ok(), Some(false), "the ask was answered none");
    assert!(pause.is_some(), "the quiet set may still be paused");
}
