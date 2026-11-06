//! Where an accepted steer went, decided from what the ledger saw (`M242_002`).
//!
//! A live durable-task run claims later messages from a person in its
//! conversation at its next safe boundary (#803's `pending_inputs.sql`), so
//! such a send is `folded`. A send with nothing waiting and no live run starts
//! a run of its own. Anything else waits its turn.

use afd_admission::RunFacts;
pub use afd_wire::event::SteerDisposition;

use super::{ACTOR_MACHINE, is_steer_actor};

/// The disposition and, for a fold, the run it joins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// Where the message went.
    pub disposition: SteerDisposition,
    /// The run's event, when `disposition` is `folded`.
    pub folded_into_event_id: Option<String>,
}

/// Names where a steer from `actor` went, given the run the ledger saw.
///
/// A person's send is the only kind a run claims mid-run: #803 reads steers
/// whose actor is a person's, never the machine's, in the run's conversation.
#[must_use]
pub fn of(run: &RunFacts, actor: &str) -> Placed {
    let person = is_steer_actor(actor) && actor != ACTOR_MACHINE;
    if let Some(live) = &run.live_run
        && person
        && live.takes_input
        && live.same_conversation
    {
        return Placed {
            disposition: SteerDisposition::Folded,
            folded_into_event_id: Some(live.event_id.clone()),
        };
    }
    let disposition = if run.live_run.is_none() && !run.earlier_waiting {
        SteerDisposition::Started
    } else {
        SteerDisposition::Queued
    };
    Placed {
        disposition,
        folded_into_event_id: None,
    }
}

#[cfg(test)]
mod tests {
    use afd_admission::{LiveRun, RunFacts};

    use super::{SteerDisposition, of};
    use crate::steer::{ACTOR_MACHINE, steer_actor};

    const RUN_EVENT: &str = "0190d5c2-7e1a-7000-8000-00000000e001";

    fn live_task() -> RunFacts {
        RunFacts {
            live_run: Some(LiveRun {
                event_id: RUN_EVENT.to_owned(),
                takes_input: true,
                same_conversation: true,
            }),
            earlier_waiting: true,
        }
    }

    fn with_run(change: impl FnOnce(&mut LiveRun)) -> RunFacts {
        let mut facts = live_task();
        if let Some(live) = facts.live_run.as_mut() {
            change(live);
        }
        facts
    }

    /// The truth table (`M242_002` Dimension 4.1).
    #[test]
    fn disposition_follows_the_run_state() {
        let person = steer_actor("user_2folder");
        let folded = of(&live_task(), &person);
        assert_eq!(folded.disposition, SteerDisposition::Folded);
        assert_eq!(folded.folded_into_event_id.as_deref(), Some(RUN_EVENT));

        assert_eq!(
            of(&RunFacts::default(), &person).disposition,
            SteerDisposition::Started
        );

        let plain = with_run(|live| live.takes_input = false);
        assert_eq!(of(&plain, &person).disposition, SteerDisposition::Queued);

        let other_thread = with_run(|live| live.same_conversation = false);
        assert_eq!(
            of(&other_thread, &person).disposition,
            SteerDisposition::Queued
        );

        assert_eq!(
            of(&live_task(), ACTOR_MACHINE).disposition,
            SteerDisposition::Queued,
            "a machine's send is never claimed mid-run"
        );

        let waiting = RunFacts {
            earlier_waiting: true,
            ..RunFacts::default()
        };
        assert_eq!(of(&waiting, &person).disposition, SteerDisposition::Queued);
    }
}
