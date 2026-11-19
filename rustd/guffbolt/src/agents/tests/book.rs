//! The book, read directly: a start against a tool the parent lacks, the
//! depth cap and the caps; a pause; a census restored; and the answers and
//! refusals the model reads.

use tokio_util::sync::CancellationToken;

use super::super::{
    Agents, Census, Counted, Kind, Listed, Parent, Refused, State, Status, not_found,
};
use super::{CAPS, Names, held, root};
use crate::tool::{ErrorCode, ToolOutput};

/// An id the book never gave.
const NEVER_STARTED: u64 = 7;

/// Ending an id with no child answers the status given, and keeps no child
/// for it: there is no earlier end to keep, and nothing to list.
#[test]
fn ending_an_id_with_no_child_answers_the_status_given() {
    let (agents, _requests) = Agents::<Names>::new(CAPS);

    let done = agents.ended(NEVER_STARTED, Status::Done("an answer".to_owned()));
    let interrupted = agents.ended(NEVER_STARTED, Status::Interrupted);

    assert_eq!(done, Kind::Done);
    assert_eq!(interrupted, Kind::Interrupted, "no earlier end was kept");
    assert_eq!(agents.list(), Vec::<Listed>::new());
    assert_eq!(agents.calls(NEVER_STARTED), 0);
}

#[test]
fn a_name_the_parent_lacks_refuses_the_child_and_books_nothing() {
    let (agents, requests) = Agents::new(CAPS);
    let (stop, tools) = (CancellationToken::new(), held());
    let asked = ["read".to_owned(), "write".to_owned()];

    let refused = agents.start(
        root(&stop, &tools),
        "copy".to_owned(),
        Some(asked.as_slice()),
    );

    assert_eq!(refused, Err(Refused::NotHeld("write".to_owned())));
    assert_eq!(agents.list(), Vec::<Listed>::new());
    assert!(requests.is_empty(), "nothing queued for the root");
    let next = agents.start(root(&stop, &tools), "copy".to_owned(), None);
    assert_eq!(next, Ok(1), "the refusal took no id");
}

/// A child below the depth cap holds every tool of its parent's; one at the
/// cap holds none that starts a child.
#[test]
fn a_child_at_the_depth_cap_holds_no_tool_that_starts_a_child() {
    let (agents, mut requests) = Agents::new(CAPS);
    let (stop, tools) = (CancellationToken::new(), held());
    let below_cap = root(&stop, &tools);
    let at_cap = Parent {
        depth: CAPS.depth - 1,
        ..root(&stop, &tools)
    };

    assert_eq!(agents.start(below_cap, "near".to_owned(), None), Ok(1));
    assert_eq!(agents.start(at_cap, "far".to_owned(), None), Ok(2));

    let near = requests
        .starts
        .try_recv()
        .map(|start| (start.depth, start.tools));
    let far = requests
        .starts
        .try_recv()
        .map(|start| (start.depth, start.tools));
    assert_eq!(near, Ok((CAPS.depth - 1, held())));
    assert_eq!(far, Ok((CAPS.depth, Names(vec!["read"]))));
}

#[test]
fn the_caps_refuse_a_child_past_the_running_or_the_started_count() {
    let (agents, _requests) = Agents::new(CAPS);
    let (stop, tools) = (CancellationToken::new(), held());
    let start = |task: &str| agents.start(root(&stop, &tools), task.to_owned(), None);

    assert_eq!(start("one"), Ok(1));
    assert_eq!(start("two"), Ok(2));
    assert_eq!(start("three"), Err(Refused::Capped), "two may run at once");
    assert_eq!(agents.interrupt(1), Some(Kind::Interrupted));
    assert_eq!(start("three"), Ok(3), "an interrupted child frees its slot");
    assert_eq!(agents.interrupt(2), Some(Kind::Interrupted));
    assert_eq!(
        start("four"),
        Err(Refused::Capped),
        "three may start in all"
    );
    assert_eq!(agents.interrupt(NEVER_STARTED), None, "no such child");
}

#[test]
fn a_pause_refuses_every_start_until_it_is_dropped() {
    let (agents, requests) = Agents::new(CAPS);
    let (stop, tools) = (CancellationToken::new(), held());
    let start = || agents.start(root(&stop, &tools), "inspect".to_owned(), None);

    assert!(
        agents.try_pause(|| false).is_none(),
        "a child's future still unwinding refuses a pause"
    );
    let pause = agents.try_pause(|| true);
    assert!(pause.is_some());
    assert_eq!(start(), Err(Refused::Capped));
    assert!(agents.try_pause(|| true).is_none(), "one pause holds them");

    drop(pause);
    assert_eq!(start(), Ok(1));
    assert!(!requests.is_empty(), "the start reaches the root");
    assert!(
        agents.try_pause(|| true).is_none(),
        "a running child refuses a pause"
    );
}

#[tokio::test]
async fn a_census_restores_ended_children_and_reads_only_under_its_own_pause() {
    let census = Census {
        next: 2,
        started: 2,
        children: vec![
            Counted {
                id: 1,
                depth: 1,
                calls: 7,
                status: Status::Done("kept".to_owned()),
            },
            Counted {
                id: 2,
                depth: 2,
                calls: 4,
                status: Status::Interrupted,
            },
        ],
    };
    let (agents, requests) = Agents::<Names>::restored(CAPS, census.clone());
    let (other, _other_requests) = Agents::<Names>::new(CAPS);

    assert!(requests.is_empty(), "no restored child starts again");
    let kept = Status::Done("kept".to_owned());
    assert_eq!(agents.wait(1, None).await, Some(kept));
    assert_eq!(
        agents.send(1, "late".to_owned()),
        Some(false),
        "it takes no input"
    );
    let foreign = other.try_pause(|| true);
    let read = foreign.as_ref().and_then(|pause| agents.census(pause));
    assert_eq!(read, None, "another book's pause reads nothing");
    let pause = agents.try_pause(|| true);
    assert_eq!(
        pause.as_ref().and_then(|pause| agents.census(pause)),
        Some(census)
    );

    drop(pause);
    let (stop, tools) = (CancellationToken::new(), held());
    let next = agents.start(root(&stop, &tools), "more".to_owned(), None);
    assert_eq!(next, Ok(3), "ids and counts go on from the census");
}

#[test]
fn a_state_carries_an_answer_once_done_and_a_detail_once_failed() {
    let done = Status::Done("x".to_owned());
    let failed = Status::Failed("y".to_owned());
    let read = |state: State<'_>| serde_json::to_string(&state).ok();

    let answered = Some(r#"{"status":"done","answer":"x"}"#.to_owned());
    assert_eq!(read(State::of(&done)), answered);
    let detailed = Some(r#"{"status":"failed","detail":"y"}"#.to_owned());
    assert_eq!(read(State::of(&failed)), detailed);
    let named = Some(r#"{"status":"running"}"#.to_owned());
    assert_eq!(read(State::named(Kind::Running)), named);
}

#[test]
fn a_refusal_reads_its_code_and_why() {
    let not_held = ToolOutput::from(Refused::NotHeld("write".to_owned()));
    let capped = ToolOutput::from(Refused::Capped);

    let held_text = "[child_tool_not_held] write is not one of this run's tools";
    assert_eq!(not_held.text, held_text);
    assert_eq!(not_held.error_code, Some(ErrorCode::CHILD_TOOL_NOT_HELD));
    let capped_text = "[child_cap_reached] as many children run, or were started, as one run \
                       may; wait for one to end";
    assert_eq!(capped.text, capped_text);
    assert_eq!(capped.error_code, Some(ErrorCode::CHILD_CAP_REACHED));
    let missing = not_found(9);
    assert_eq!(
        missing.text,
        "[child_not_found] no child of this run has id 9"
    );
    assert_eq!(missing.error_code, Some(ErrorCode::CHILD_NOT_FOUND));
}
