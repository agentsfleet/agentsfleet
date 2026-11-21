//! What the setup loop's suites share: a draft's run, the setup it
//! proposes, and a fake of the daemon's context verb.
#![expect(
    clippy::expect_used,
    reason = "test fixture: a payload that does not parse is a broken test"
)]

use std::sync::{Mutex, PoisonError};

use af_core::money::NANOS_PER_USD;
use af_providers::Connect;
use af_wire::activity::ActivityFrame;
use af_wire::context::{ContextLoadResponse, ContextSaveRequest, ContextSaveResponse};
use af_wire::run::RunPayload;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{SetupContext, SetupOutput, SetupRun};
use crate::Meter;
use crate::engine::EventSink;

/// The draft the run's turn is on, which a checkpoint's scope names.
const DRAFT: &str = "019f0000-0000-7000-8000-0000000000d1";
/// The scope the daemon assigns the draft's checkpoints.
pub(super) const SCOPE: &str = "draft:019f0000-0000-7000-8000-0000000000d1";
/// The fence the run holds the draft's thread at.
pub(super) const FENCE: u64 = 3;
/// What the person asked in the turn the run answers.
pub(super) const ASKED: &str = "Check the support queue every morning at nine.";
/// The head candidate an edit names.
pub(super) const HEAD: &str = "019f0000-0000-7000-8000-0000000000c1";
const AT: &str = "1970-01-01T00:00:00.001Z";
/// What the model answers once it proposed.
pub(super) const ANSWER: &str = "A daily check at nine, UTC.";

/// A setup run's payload: `history` as (message, answer, sequence), oldest
/// first; `context_tokens` the model's window, zero for one never fitted;
/// `candidate` the head the turn edits.
pub(super) fn setup_payload(
    history: &[(&str, &str, u64)],
    context_tokens: u32,
    candidate: Option<&Value>,
) -> RunPayload<'static> {
    let turns: Vec<Value> = history
        .iter()
        .map(|(message, answer, sequence)| {
            json!({"sequence": sequence, "message": message, "answer": answer})
        })
        .collect();
    let document = json!({
        "run_id": "019f0000-0000-7000-8000-0000000000a1", "fencing_token": FENCE,
        "run_expires_at": AT,
        "event": {"turn_id": "019f0000-0000-7000-8000-0000000000b1", "fleet_id": DRAFT,
            "workspace_id": "ws-1", "actor": "user-1", "event_type": "chat",
            "request_json": json!({"message": ASKED}).to_string(), "created_at": AT},
        "policy": {"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": ["propose_setup"], "secrets_map": null, "mintable": [],
            "provider": "anthropic", "api_key": crate::fixture::API_KEY,
            "inference_host": "h", "base_url": null, "repository_binding": null,
            "http_origin_policies": [], "model_context_tokens": context_tokens,
            "inference": {"api": "messages", "base_url": "https://models.example.com/v1",
                "max_output_tokens": 100},
            "context": {"tool_window": 0, "memory_checkpoint_every": 0,
                "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0}},
        "instructions": "Shape the person's fleet.", "history": turns,
        "resume_hold": false, "inputs_waiting": false,
        "owner": {"kind": "draft", "draft_id": DRAFT,
            "setup": {"sequence": history.len() + 1, "candidate": candidate,
                "save_requested": false, "tool": {}}}
    });
    // Leaked so the borrowed payload lives as long as the test that reads it.
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).expect("a fixture payload parses")
}

/// A support setup whose one cron trigger fires on `schedule`.
pub(super) fn document(schedule: &str) -> Value {
    json!({
        "schema_version": 1,
        "description": "Checks the support queue.",
        "instructions_md": "Answer directly.",
        "config": {"name": "support-triage", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": schedule, "timezone": "UTC",
                "message": "Daily check"}],
            "tools": [],
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    })
}

/// A first setup whose trigger fires on `schedule`.
pub(super) fn initialize(schedule: &str) -> Value {
    json!({"proposal": {"kind": "initialize", "definition": document(schedule)}})
}

/// The draft's context, as a fake of the daemon's context verb serves it.
#[derive(Debug, Default)]
pub(super) struct Context {
    /// What a load answers; none refuses it.
    pub(super) loaded: Option<ContextLoadResponse>,
    /// The code every save is refused with, when saves are refused:
    /// `RUN_STALE_FENCING_TOKEN` is a superseded run's.
    pub(super) refuse_saves: Option<af_core::error_code::ErrorCode>,
    /// Every save the run asked for, in order.
    pub(super) saves: Mutex<Vec<ContextSaveRequest>>,
}

impl Context {
    /// A draft with no saved context yet.
    pub(super) fn empty() -> Self {
        Self::stored(0, None)
    }

    /// A draft whose stored context is `checkpoint` at `version`.
    pub(super) fn stored(version: u64, checkpoint: Option<Value>) -> Self {
        Self {
            loaded: Some(ContextLoadResponse {
                scope: SCOPE.to_owned(),
                resource_version: version,
                checkpoint,
            }),
            ..Self::default()
        }
    }

    /// The saves asked for so far.
    pub(super) fn saved(&self) -> Vec<ContextSaveRequest> {
        self.saves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl SetupContext for Context {
    async fn load(&self) -> crate::Result<ContextLoadResponse> {
        self.loaded
            .clone()
            .ok_or_else(|| crate::Error::setup_context(af_providers::Error::refused(409)))
    }

    async fn save(&self, request: ContextSaveRequest) -> crate::Result<ContextSaveResponse> {
        if let Some(code) = self.refuse_saves {
            return Err(crate::Error::setup_context(afr_memory::Error::unanswered(
                code,
            )));
        }
        let resource_version = request.resource_version + 1;
        self.saves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        Ok(ContextSaveResponse { resource_version })
    }
}

/// Runs `run` to its end over `connect`, its frames dropped.
pub(super) async fn run_setup(
    connect: &dyn Connect,
    run: &RunPayload<'_>,
    context: &dyn SetupContext,
) -> crate::Result<SetupOutput> {
    let dropped = |_frame: ActivityFrame<'static>| {};
    run_setup_heard(connect, run, context, &dropped).await
}

/// Runs `run` to its end over `connect`, its frames sent to `events`.
pub(super) async fn run_setup_heard(
    connect: &dyn Connect,
    run: &RunPayload<'_>,
    context: &dyn SetupContext,
    events: &dyn EventSink,
) -> crate::Result<SetupOutput> {
    let meter = Meter::default();
    let stop = CancellationToken::new();
    let setup = SetupRun {
        run,
        context,
        events,
        meter: &meter,
        stop: &stop,
    };
    super::run(connect, setup).await
}
