//! Where an accepted steer went, decided from what the ledger saw.
//!
//! A live run claims the messages its fleet receives at its next model call:
//! a durable task a person's messages in its conversation
//! (`pending_inputs.sql`), any other run every message, up to the first parked
//! or task-bound one (`claim_inputs.sql`). Such a send is `folded`. A send with
//! nothing waiting and no live run starts a run of its own. Anything else
//! waits its turn.

pub use af_wire::event::SteerDisposition;
use afd_admission::RunFacts;

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
/// A durable task reads steers whose actor is a person's, never the
/// machine's, in its conversation. Any other run folds every message unless
/// one it must not pass waits ahead of it.
///
/// `message_id` is the admission this answer is for. A retry of the send that
/// started the live run meets that run as its head, and a message cannot fold
/// into the run it began, so it is `started` again. A fresh message never
/// matches: the run it sees was admitted before it.
#[must_use]
pub fn of(run: &RunFacts, actor: &str, message_id: &str) -> Placed {
    if let Some(live) = &run.live_run {
        if live.event_id == message_id {
            return without_run(SteerDisposition::Started);
        }
        let folds = if live.durable_task {
            is_steer_actor(actor) && actor != ACTOR_MACHINE && live.same_conversation
        } else {
            !run.earlier_waiting
        };
        if folds {
            return Placed {
                disposition: SteerDisposition::Folded,
                folded_into_event_id: Some(live.event_id.clone()),
            };
        }
    }
    without_run(if run.live_run.is_none() && !run.earlier_waiting {
        SteerDisposition::Started
    } else {
        SteerDisposition::Queued
    })
}

/// A placement that joins no run.
const fn without_run(disposition: SteerDisposition) -> Placed {
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
    /// A message other than the one that started the run.
    const LATER: &str = "0190d5c2-7e1a-7000-8000-00000000e002";

    fn live_task() -> RunFacts {
        RunFacts {
            live_run: Some(LiveRun {
                event_id: RUN_EVENT.to_owned(),
                durable_task: true,
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

    #[test]
    fn disposition_follows_the_run_state() {
        let person = steer_actor("user_2folder");
        let folded = of(&live_task(), &person, LATER);
        assert_eq!(folded.disposition, SteerDisposition::Folded);
        assert_eq!(folded.folded_into_event_id.as_deref(), Some(RUN_EVENT));

        assert_eq!(
            of(&RunFacts::default(), &person, LATER).disposition,
            SteerDisposition::Started
        );

        let other_thread = with_run(|live| live.same_conversation = false);
        assert_eq!(
            of(&other_thread, &person, LATER).disposition,
            SteerDisposition::Queued,
            "a durable task reads only its own conversation"
        );

        assert_eq!(
            of(&live_task(), ACTOR_MACHINE, LATER).disposition,
            SteerDisposition::Queued,
            "a durable task never claims a machine's send"
        );

        let plain = with_run(|live| live.durable_task = false);
        let plain_clear = RunFacts {
            earlier_waiting: false,
            ..plain.clone()
        };
        assert_eq!(
            of(&plain_clear, ACTOR_MACHINE, LATER).disposition,
            SteerDisposition::Folded,
            "any other run folds every send, a machine's included"
        );
        assert_eq!(
            of(
                &RunFacts {
                    live_run: plain_clear.live_run.clone().map(|live| LiveRun {
                        same_conversation: false,
                        ..live
                    }),
                    ..plain_clear
                },
                &person,
                LATER
            )
            .disposition,
            SteerDisposition::Folded,
            "whatever conversation it came from"
        );
        assert_eq!(
            of(&plain, &person, LATER).disposition,
            SteerDisposition::Queued,
            "a parked or task-bound message ahead is never passed"
        );

        let waiting = RunFacts {
            earlier_waiting: true,
            ..RunFacts::default()
        };
        assert_eq!(
            of(&waiting, &person, LATER).disposition,
            SteerDisposition::Queued
        );
    }

    /// An actor that is not a steer's at all, such as a webhook's, is not a
    /// person: a durable task leaves its message for a later run even in the
    /// task's own conversation, where any other run still folds it.
    #[test]
    fn a_durable_task_leaves_a_message_whose_actor_is_no_steer() {
        for actor in ["webhook:github", "cron:nightly", ""] {
            assert_eq!(
                of(&live_task(), actor, LATER).disposition,
                SteerDisposition::Queued,
                "{actor:?} is not a person"
            );
        }
        let plain_clear = RunFacts {
            earlier_waiting: false,
            ..with_run(|live| live.durable_task = false)
        };
        assert_eq!(
            of(&plain_clear, "webhook:github", LATER).disposition,
            SteerDisposition::Folded
        );
    }

    /// A retry of the send that started the live run meets that run as its own
    /// head. It never folds into the run it began, for whoever sent it and
    /// whatever kind of run it is.
    #[test]
    fn a_retry_of_the_send_that_started_the_run_is_started_again() {
        let person = steer_actor("user_2folder");
        let plain_clear = RunFacts {
            earlier_waiting: false,
            ..with_run(|live| live.durable_task = false)
        };
        for (facts, actor) in [
            (live_task(), person.as_str()),
            (live_task(), ACTOR_MACHINE),
            (plain_clear.clone(), person.as_str()),
            (plain_clear, ACTOR_MACHINE),
        ] {
            let placed = of(&facts, actor, RUN_EVENT);
            assert_eq!(placed.disposition, SteerDisposition::Started, "{actor}");
            assert_eq!(placed.folded_into_event_id, None, "{actor}");
        }
    }
}
