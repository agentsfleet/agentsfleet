//! Children polled beside the root: a child's answer reaches the call that
//! handed it its task, a child interrupted mid-loop ends once, and ending the
//! set interrupts a child still running.

use tokio::task::yield_now;
use tokio_util::sync::CancellationToken;

use super::super::{Agents, Children, Kind, Refused};
use super::{CAPS, HANG, Heard, Run, STOPS, held, root};
use crate::tool::{ErrorCode, ToolOutput};

#[tokio::test]
async fn a_child_runs_beside_its_parent_and_its_answer_is_the_delegated_output() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let parent = async {
        let id = run
            .agents
            .start(root(&stop, &tools), "summarise".to_owned(), None)?;
        Ok::<_, Refused>(run.agents.delegated(id).await)
    };

    let output = children.beside(&run, parent).await;

    assert_eq!(output, Ok(ToolOutput::succeeded("summarise")));
    let heard = [Heard::Started(1, 1), Heard::Ended(1, Kind::Done, 0)];
    assert_eq!(*run.heard(), heard);
    assert!(
        children.end_all(&run.agents).is_some(),
        "a quiet set pauses"
    );
}

/// Interrupting a child cancels its stop: its loop sees it and ends, and the
/// child ends once, interrupted.
#[tokio::test]
async fn an_interrupted_child_sees_its_stop_and_ends_once() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let parent = async {
        let id = run
            .agents
            .start(root(&stop, &tools), STOPS.to_owned(), None)?;
        // Twice each time: the root starts the child, then polls it.
        yield_now().await;
        yield_now().await;
        let interrupted = run.agents.interrupt(id);
        let delegated = run.agents.delegated(id).await;
        yield_now().await;
        yield_now().await;
        Ok::<_, Refused>((interrupted, delegated))
    };

    let ended = children.beside(&run, parent).await;

    let delegated = ToolOutput::failed(
        ErrorCode::INTERRUPTED,
        "child 1 was interrupted before it answered",
    );
    assert_eq!(ended, Ok((Some(Kind::Interrupted), delegated)));
    let heard = [Heard::Started(1, 1), Heard::Ended(1, Kind::Interrupted, 0)];
    assert_eq!(*run.heard(), heard, "it ends once");
}

#[tokio::test]
async fn ending_the_set_interrupts_a_child_still_running_and_frees_its_slot() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let parent = async {
        let started = run.agents.start(root(&stop, &tools), HANG.to_owned(), None);
        // Twice: the root starts the child, then polls it once.
        yield_now().await;
        yield_now().await;
        started
    };

    let started = children.beside(&run, parent).await;

    assert_eq!(started, Ok(1));
    assert_eq!(*run.heard(), [Heard::Started(1, 1)], "it still runs");
    let pause = children.end_all(&run.agents);
    let heard = [Heard::Started(1, 1), Heard::Ended(1, Kind::Interrupted, 0)];
    assert_eq!(*run.heard(), heard);
    assert!(
        pause.is_some(),
        "its slot is free and nothing is left to run"
    );
    let listed = run.agents.list().first().map(|child| child.status);
    assert_eq!(listed, Some(Kind::Interrupted));
}

/// A child asked for and never started, because the run ended first, ends
/// interrupted with the set, and was never heard to start.
#[tokio::test]
async fn a_child_queued_when_the_set_ends_ends_interrupted_unheard() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let parent = async { run.agents.start(root(&stop, &tools), HANG.to_owned(), None) };

    let started = children.beside(&run, parent).await;
    let pause = children.end_all(&run.agents);

    assert_eq!(started, Ok(1));
    assert!(run.heard().is_empty(), "it never started");
    assert!(pause.is_some());
    let listed = run.agents.list().first().map(|child| child.status);
    assert_eq!(listed, Some(Kind::Interrupted));
}
