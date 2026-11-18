//! Root and child state start under the current lease's authority.

use super::{Harness, Shared, context, publication};
use crate::{
    context::Checkpoints,
    events::Live,
    nested::{Guard, Seat, Tether},
    offer,
    router::Router,
};
use af_providers::Message;
use afr_tools::Selection;

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
            group_start: if shared.recovered_task.is_some() {
                0
            } else {
                opening.len()
            },
            shared,
            depth: 0,
            stop: shared.stop.clone(),
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
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
            messages: opening,
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
            root.specs.push(publication::spec());
        }
        root
    }

    /// A child loop over `selection`, opening with its seat's task. Its text
    /// goes to no frame: its parent reads it through the call that started
    /// it. It writes no checkpoint: the push before the report carries what
    /// it stored.
    pub(crate) fn child(
        shared: &'s Shared<'run>,
        selection: &'s Selection<'run>,
        seat: Seat,
        guard: &'s Guard<'s, 'run>,
    ) -> Self {
        Self {
            task: None,
            incorporated_input_sequence: 0,
            reconciled_effect_ordinal: 0,
            confirmed_effect_watermark: 0,
            task_report: None,
            group_start: 1,
            shared,
            depth: seat.depth,
            stop: seat.stop,
            selection,
            router: Router::new(selection, shared.executor),
            specs: offer::specs(selection),
            hosted: offer::hosted(selection),
            live: Live::silent(shared.scrub, shared.started),
            checkpoints: Checkpoints::never(),
            context: context::State::new(0),
            driver: guffbolt::DriveState::default(),
            messages: vec![Message::User(shared.scrub.clean(seat.task).into_inner())],
            child: Some(Tether {
                guard,
                input: seat.input,
            }),
            folded_through: 0,
            folded_inputs: Vec::new(),
            turn_checkpoint: None,
            claim_answered: false,
        }
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
                    replay: af_providers::Replay::default(),
                },
            ]
        })
        .collect()
}
