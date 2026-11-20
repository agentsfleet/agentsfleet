//! Waiting for the set to be quiet: an ask that waits is answered with a
//! pause once every running child has ended and its end is recorded, and
//! with none when a pause is already held or the set has ended.

use std::time::Duration;

use tokio::task::yield_now;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::{Agents, Census, Children, Counted, Kind, Refused, Status};
use super::{CAPS, HANG, Heard, Names, Run, SLEEPS, held, root};

/// Longer than any case runs, so an ask that is never answered fails its
/// case.
const GUARD: Duration = Duration::from_secs(3_600);

#[tokio::test(start_paused = true)]
async fn a_quiet_ask_waits_for_a_running_child_and_pauses_over_its_answer() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let parent = async {
        let id = run
            .agents
            .start(root(&stop, &tools), SLEEPS.to_owned(), None)?;
        let refused = run.agents.pause().await.is_none();
        let paused = run.agents.quiet().await;
        let census = paused.as_ref().and_then(|pause| run.agents.census(pause));
        Ok::<_, Refused>((id, refused, census))
    };

    let ended = timeout(GUARD, children.beside(&run, parent)).await;

    let counted = Counted {
        id: 1,
        depth: 1,
        calls: 0,
        status: Status::Done(SLEEPS.to_owned()),
    };
    let census = Census {
        next: 1,
        started: 1,
        children: vec![counted],
    };
    assert_eq!(
        ended,
        Ok(Ok((1, true, Some(census)))),
        "a plain ask is refused while the child runs; a quiet one waits for its answer"
    );
    let heard = [Heard::Started(1, 1), Heard::Ended(1, Kind::Done, 0)];
    assert_eq!(*run.heard(), heard);
}

#[tokio::test(start_paused = true)]
async fn a_quiet_ask_while_a_pause_is_held_answers_none() {
    let (agents, requests) = Agents::<Names>::new(CAPS);
    let run = Run::new(agents);
    let mut children = Children::new(requests);
    let parent = async {
        let held = run.agents.pause().await;
        let second = run.agents.quiet().await;
        (held.is_some(), second.is_some())
    };

    let ended = timeout(GUARD, children.beside(&run, parent)).await;

    assert_eq!(ended, Ok((true, false)), "one pause holds the children");
}

#[tokio::test(start_paused = true)]
async fn a_quiet_ask_once_the_set_has_ended_answers_none() {
    let (agents, requests) = Agents::<Names>::new(CAPS);
    let mut children = Children::new(requests);
    drop(children.end_all(&agents));

    let answered = timeout(GUARD, agents.quiet()).await;

    assert!(
        matches!(answered, Ok(None)),
        "no poller is left: {answered:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn ending_the_set_answers_a_quiet_ask_still_waiting_with_none() {
    let (agents, requests) = Agents::new(CAPS);
    let run = Run::new(agents);
    let (stop, tools) = (CancellationToken::new(), held());
    let mut children = Children::new(requests);
    let mut quiet = Box::pin(run.agents.quiet());
    let parent = async {
        let started = run.agents.start(root(&stop, &tools), HANG.to_owned(), None);
        // One poll sends the ask; the set reads it while the child that never
        // answers keeps it waiting.
        let unanswered = timeout(Duration::ZERO, &mut quiet).await.is_err();
        yield_now().await;
        yield_now().await;
        (started, unanswered)
    };

    let ended = children.beside(&run, parent).await;
    let before = format!("{children:?}");
    drop(children.end_all(&run.agents));
    let answered = timeout(GUARD, quiet).await;

    assert_eq!(ended, (Ok(1), true));
    assert_eq!(
        before, "Children { running: 1, queued: false, waiting: 1 }",
        "the ask waited in the set"
    );
    assert!(matches!(answered, Ok(None)), "{answered:?}");
}
