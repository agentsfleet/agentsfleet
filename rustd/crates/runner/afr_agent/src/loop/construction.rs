//! Root and child state start under the current run's authority.

use super::{EVENT_CHILD_STARTED, Harness, Shared, context, publication};
use crate::{context::Checkpoints, events::Live, offer, router::Router};
use af_wire::run::{EarlierCall, Turn};
use af_wire::tool_trace::CALL_ID_SEPARATOR;
use guffbolt::agents::Seat;
use guffbolt::{Call, Message, Replay, Transcript};

/// What opens an earlier call's id, so it never meets an id a provider mints
/// in this run.
const EARLIER_CALL_PREFIX: &str = "earlier_";

impl<'s, 'run> Harness<'s, 'run> {
    /// The run's root loop, over every tool the run was offered, opening
    /// with the fleet's earlier turns and then the event's message; `saved`
    /// is the thread's summary a fleet chat read, which its compactions save.
    pub(super) fn root(
        shared: &'s Shared<'run>,
        opening: Vec<Message>,
        saved: Option<context::Saved>,
    ) -> Self {
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
            id: 0,
            depth: 0,
            stop: shared.stop.clone(),
            selection,
            router: Router::new(offer::tools(selection, &shared.backed), selection),
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
            folded_through: 0,
            folded_inputs: Vec::new(),
            turn_checkpoint: shared.payload.turn_resume.clone(),
            assignment_answered: !shared.payload.inputs_waiting,
        };
        root.context.saved = saved;
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

    /// One of a child's turns, over the transcript `guffbolt` keeps for it,
    /// whose input is its task. It holds every tool the run was offered, and
    /// `guffbolt` offers it only those its parent gave it. Its text goes to no
    /// frame: its parent reads it through the call that waits on it. It writes no
    /// checkpoint: the push before the report carries what it stored. It
    /// never publishes, so it holds `propose_change` unoffered.
    pub(crate) fn child(shared: &'s Shared<'run>, seat: Seat) -> Self {
        let Seat {
            id,
            depth,
            stop,
            transcript,
        } = seat;
        let run_id = shared.run_id;
        let child_id = id;
        let event = EVENT_CHILD_STARTED;
        tracing::info!(run_id, child_id, depth, event);
        let selection = shared.selection;
        let mut child = Self {
            task: None,
            incorporated_input_sequence: 0,
            reconciled_effect_ordinal: 0,
            confirmed_effect_watermark: 0,
            task_report: None,
            shared,
            id,
            depth,
            stop,
            selection,
            router: Router::new(offer::tools(selection, &shared.backed), selection),
            hosted: offer::hosted(selection),
            live: Live::silent(shared.scrub, shared.started),
            checkpoints: Checkpoints::never(),
            context: context::State::new(0),
            driver: guffbolt::DriveState::default(),
            transcript,
            folded_through: 0,
            folded_inputs: Vec::new(),
            turn_checkpoint: None,
            assignment_answered: false,
        };
        child.router.hold(Box::new(publication::Propose));
        child
    }
}

/// The root's opening ahead of the event's message: the thread's saved
/// summary when `saved` holds one, then each earlier turn it does not cover.
/// A turn is what was asked, each call its run made and what came back when
/// `calls` says the run is offered tools, and what was answered. Every text
/// passes `scrub`: a secret said in an earlier turn is still a secret.
/// `saved` learns where each turn sits.
///
/// # Errors
/// The saved summary cannot be quoted.
pub(super) fn opening(
    scrub: &af_secrets::Scrub,
    mut saved: Option<&mut context::Saved>,
    history: &[Turn<'_>],
    calls: bool,
) -> af_context::Result<Vec<Message>> {
    let mut messages = Vec::new();
    if let Some(summary) = saved.as_deref().and_then(context::Saved::summary) {
        messages.push(Message::User(summary.context()?));
    }
    for turn in history {
        if saved.as_deref().is_some_and(|kept| kept.covers(turn)) {
            continue;
        }
        let start = messages.len();
        messages.push(Message::User(
            scrub.clean(turn.message.to_string()).into_inner(),
        ));
        if calls {
            for call in &turn.calls {
                messages.extend(earlier_call(scrub, call));
            }
        }
        messages.push(Message::Assistant {
            text: scrub.clean(turn.answer.to_string()).into_inner(),
            calls: Vec::new(),
            replay: Replay::default(),
        });
        if let Some(kept) = saved.as_deref_mut() {
            kept.placed(turn.sequence, start..messages.len());
        }
    }
    Ok(messages)
}

/// One earlier call as `guffbolt` keeps a call: the step that made it, then
/// its result, under an id the provider accepts, made from the thread's
/// `{fence}:{n}`, which is unique in the fleet's thread.
fn earlier_call(scrub: &af_secrets::Scrub, call: &EarlierCall<'_>) -> [Message; 2] {
    let id = format!(
        "{EARLIER_CALL_PREFIX}{}",
        call.call_id.replace(CALL_ID_SEPARATOR, "_")
    );
    let arguments = scrub
        .clean_json(serde_json::Value::Object(call.arguments.clone()))
        .into_inner();
    [
        Message::Assistant {
            text: String::new(),
            calls: vec![Call {
                id: id.clone(),
                name: call.tool.to_string(),
                arguments,
            }],
            replay: Replay::default(),
        },
        Message::ToolResult {
            call_id: id,
            output: scrub.clean(call.output.to_string()).into_inner(),
            image: None,
        },
    ]
}
