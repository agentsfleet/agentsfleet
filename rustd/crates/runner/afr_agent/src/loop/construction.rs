//! Root and child state start under the current lease's authority.

use super::{Harness, Shared, context, publication};
use crate::{context::Checkpoints, events::Live, offer, router::Router};
use afr_tools::Selection;
use guffbolt::agents::Seat;
use guffbolt::{Message, Replay, Transcript};

impl<'s, 'run> Harness<'s, 'run> {
    /// The run's root loop, over every tool the lease was offered, opening
    /// with the fleet's earlier turns and then the event's message.
    pub(super) fn root(shared: &'s Shared<'run>, opening: Vec<Message>) -> Self {
        let selection = shared.selection;
        let mut root = Self {
            task: shared.prepared_task.cloned(),
            incorporated_input_sequence: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.covered_input_sequence),
            reconciled_effect_ordinal: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.confirmed_effect_watermark),
            confirmed_effect_watermark: shared
                .recovered_task
                .map_or(0, |recovery| recovery.task.confirmed_effect_watermark),
            task_report: None,
            shared,
            depth: 0,
            stop: shared.stop.clone(),
            selection,
            router: Router::new(offer::tools(selection), selection),
            hosted: offer::hosted(selection),
            live: Live::new(shared.events, shared.scrub, shared.started),
            checkpoints: Checkpoints::new(shared.context),
            context: context::State::new(
                if shared.recovered_task.is_some() || shared.recovered_turn.is_some() {
                    0
                } else {
                    opening.len().saturating_sub(1)
                },
            ),
            // A recovered task's first assessment judges its whole saved
            // history as one action group.
            transcript: if shared.recovered_task.is_some() {
                Transcript::resumed(opening)
            } else {
                Transcript::new(opening)
            },
            driver: guffbolt::DriveState::default(),
            child: None,
            folded_through: 0,
            folded_inputs: Vec::new(),
            turn_checkpoint: shared.payload.turn_resume.clone(),
            claim_answered: !shared.payload.inputs_waiting,
        };
        if root.task.is_some() {
            root.live.hide_reasoning();
        }
        if root.publication_allowed() {
            root.router.offer(Box::new(publication::Propose));
        } else {
            root.router.hold(Box::new(publication::Propose));
        }
        root
    }

    /// A child loop over `selection`, opening with its seat's task. Its text
    /// goes to no frame: its parent reads it through the call that started
    /// it. It writes no checkpoint: the push before the report carries what
    /// it stored. It never publishes, so it holds `propose_change` unoffered.
    pub(crate) fn child(
        shared: &'s Shared<'run>,
        selection: &'s Selection<'run>,
        seat: Seat<'s>,
    ) -> Self {
        let Seat {
            depth,
            task,
            stop,
            tether,
        } = seat;
        let mut child = Self {
            task: None,
            incorporated_input_sequence: 0,
            reconciled_effect_ordinal: 0,
            confirmed_effect_watermark: 0,
            task_report: None,
            shared,
            depth,
            stop,
            selection,
            router: Router::new(offer::tools(selection), selection),
            hosted: offer::hosted(selection),
            live: Live::silent(shared.scrub, shared.started),
            checkpoints: Checkpoints::never(),
            context: context::State::new(0),
            driver: guffbolt::DriveState::default(),
            transcript: Transcript::new(vec![Message::User(shared.scrub.clean(task).into_inner())]),
            child: Some(tether),
            folded_through: 0,
            folded_inputs: Vec::new(),
            turn_checkpoint: None,
            claim_answered: false,
        };
        child.router.hold(Box::new(publication::Propose));
        child
    }
}

pub(super) fn opening(scrub: &af_secrets::Scrub, history: Vec<(String, String)>) -> Vec<Message> {
    history
        .into_iter()
        .flat_map(|(asked, answered)| {
            [
                Message::User(scrub.clean(asked).into_inner()),
                Message::Assistant {
                    text: scrub.clean(answered).into_inner(),
                    calls: Vec::new(),
                    replay: Replay::default(),
                },
            ]
        })
        .collect()
}
