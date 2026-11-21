//! A committed draft run rendered as its assignment: what the session keeps
//! beside it and the [`RunPayload`] the runner receives.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use af_wire::{
    policy::{ExecutionPolicy, NetworkPolicy},
    run::{Owner, RunPayload, SetupCandidate, SetupWork, Turn},
    turn::{TurnEnvelope, TurnKind},
};
use afd_credential::provider::RegisteredRoute;
use afd_setup::setup_run::{SetupTurn, setup_tool};
use jiff::Timestamp;

use super::DraftTarget;
use crate::run::AssignedRun;

/// The one tool a setup run offers.
const PROPOSE_SETUP: &str = "propose_setup";

/// A committed draft run, ready to render as its assignment.
///
/// Holds the key the assignment carries, so the payload borrows it rather than
/// copying a secret into an owned string.
#[derive(Debug)]
pub struct DraftAssignment {
    run_id: Uuid7,
    fence: i64,
    expires_at: Timestamp,
    turn: SetupTurn,
    api_key: SecretString,
    inference_host: Box<str>,
    tool: serde_json::Value,
}

impl DraftAssignment {
    /// The committed run `run_id` under `fence` until `expires_at`, running
    /// `turn` with `route`'s key.
    pub(super) fn new(
        run_id: Uuid7,
        fence: i64,
        expires_at: Timestamp,
        turn: SetupTurn,
        route: RegisteredRoute,
    ) -> Self {
        let tool = setup_tool(turn.candidate.is_some());
        Self {
            run_id,
            fence,
            expires_at,
            turn,
            api_key: route.api_key,
            inference_host: route.inference_host,
            tool,
        }
    }

    /// What the session keeps beside the assignment.
    #[must_use]
    pub fn assigned(&self) -> AssignedRun {
        AssignedRun {
            run_id: self.run_id.clone(),
            expires_at: self.expires_at,
            target: None,
            reported: None,
            draft: Some(DraftTarget {
                draft_id: self.turn.draft_id.clone(),
            }),
        }
    }

    /// The assignment: a [`RunPayload`] the draft owns, borrowing the key this
    /// value holds, its oldest turns dropped only if it would not fit one
    /// run frame.
    #[must_use]
    pub fn payload(&self) -> RunPayload<'_> {
        let turn = &self.turn;
        let model = &turn.model;
        let candidate = turn.candidate.as_ref().map(|head| SetupCandidate {
            id: Cow::Owned(head.id.to_string()),
            revision: head.revision,
            definition: head.definition.clone(),
        });
        let instructions = self
            .tool
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let context_tokens = u32::try_from(model.context_window_tokens).unwrap_or_default();
        let request = serde_json::json!({ (af_wire::turn::MESSAGE): turn.user_text });
        crate::run::history::fit_frame(RunPayload {
            run_id: Cow::Owned(self.run_id.to_string()),
            fencing_token: u64::try_from(self.fence).unwrap_or_default(),
            setup_revision_id: None,
            turn_resume: None,
            run_expires_at: self.expires_at,
            event: TurnEnvelope {
                turn_id: Cow::Owned(turn.message_id.to_string()),
                // A draft's run names its draft where a fleet's names its fleet.
                fleet_id: Cow::Owned(turn.draft_id.to_string()),
                workspace_id: Cow::Owned(turn.workspace_id.to_string()),
                actor: Cow::Owned(turn.user_id.to_string()),
                event_type: TurnKind::Chat,
                request_json: Cow::Owned(request.to_string()),
                created_at: turn.message_created_at,
            },
            policy: self.policy(context_tokens),
            instructions: Cow::Borrowed(instructions),
            history: history(turn),
            resume_hold: false,
            decision: None,
            task: None,
            durable_tasks: false,
            memory: None,
            inputs_waiting: false,
            owner: Owner::Draft {
                draft_id: Cow::Owned(turn.draft_id.to_string()),
                setup: Box::new(SetupWork {
                    sequence: u64::try_from(turn.sequence).unwrap_or_default(),
                    candidate,
                    save_requested: turn.save_requested,
                    tool: self.tool.clone(),
                }),
            },
        })
    }

    /// The draft run's policy: no network, the one setup tool, and the
    /// model's key and host, borrowed from this value.
    fn policy(&self, context_tokens: u32) -> ExecutionPolicy<'_> {
        let model = &self.turn.model;
        ExecutionPolicy {
            network_policy: NetworkPolicy::default(),
            tools: vec![Cow::Borrowed(PROPOSE_SETUP)],
            secrets_map: None,
            mintable: Vec::new(),
            provider: Cow::Borrowed(&model.provider),
            inference: model.inference.clone(),
            model_context_tokens: context_tokens,
            api_key: Cow::Borrowed(self.api_key.expose()),
            inference_host: Cow::Borrowed(&self.inference_host),
            base_url: model.base_url.as_deref().map(Cow::Borrowed),
            repository_binding: None,
            http_origin_policies: Vec::new(),
            context: afd_gate::policy::context::resolve(
                None,
                afd_gate::policy::context::Overlay {
                    cap_tokens: context_tokens,
                    model: &model.model,
                },
            ),
        }
    }
}

/// The draft's earlier turns, oldest first, each named by its sequence: every
/// answered turn, whole.
fn history(turn: &SetupTurn) -> Vec<Turn<'_>> {
    turn.history
        .iter()
        .map(|earlier| Turn {
            sequence: u64::try_from(earlier.sequence).unwrap_or_default(),
            message: Cow::Borrowed(&earlier.message),
            calls: Vec::new(),
            answer: Cow::Borrowed(&earlier.answer),
        })
        .collect()
}
