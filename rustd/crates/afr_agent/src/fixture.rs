//! What the loop's suites share: a lease, a scripted provider that keeps every
//! request it was sent, and tools whose output or stall a test chooses.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use afd_wire::activity::{ActivityFrame, StreamTextKind};
use afd_wire::lease::LeasePayload;
use afr_providers::{Call, Chunk, Message, Provider, Request, Usage};
use afr_tools::{Entry, Schema, Tool, ToolContext, ToolOutput};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

/// Why every fixture lock is sound: no test panics while holding one.
const UNPOISONED: &str = "no test panics holding it";
/// Where a fixture lease's context knobs sit.
const CONTEXT_POINTER: &str = "/policy/context";

/// The provider key every fixture lease carries.
pub(crate) const API_KEY: &str = "sk-test-0123456789";
/// The static credential every fixture lease carries.
pub(crate) const GITHUB_TOKEN: &str = "ghp_fixture_token_abcdef";

/// A lease offering `tools`, with `budget` as its context knobs.
pub(crate) fn lease(tools: &[&str], budget: serde_json::Value) -> LeasePayload<'static> {
    let mut document = serde_json::json!({
        "lease_id": "lease-1", "fencing_token": 7, "lease_expires_at": 1,
        "secret_delivery": "inline",
        "event": {"event_id": "event-1", "fleet_id": "fleet-1", "workspace_id": "ws-1",
            "actor": "operator-1", "event_type": "chat",
            "request_json": "{\"message\":\"triage the failed run\"}", "created_at": 1},
        "policy": {"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": tools,
            "secrets_map": {"github": {"token": GITHUB_TOKEN, "host": "api.github.com"}},
            "mintable": [], "provider": "anthropic", "api_key": API_KEY,
            "inference_host": "h", "base_url": null, "repository_binding": null,
            "http_origin_policies": [], "context": null},
        "instructions": "Read the run.", "bundle": null
    });
    *document.pointer_mut(CONTEXT_POINTER).unwrap() = budget;
    // Leaked so the borrowed payload lives as long as the test that reads it.
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).unwrap()
}

/// Context knobs that never bound a test: no window, no cap.
pub(crate) fn unbounded() -> serde_json::Value {
    budget(0, 0)
}

/// Context knobs keeping `tool_window` results, capped at `cap` tokens.
pub(crate) fn budget(tool_window: u32, cap: u32) -> serde_json::Value {
    serde_json::json!({"tool_window": tool_window, "memory_checkpoint_every": 0,
        "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": cap})
}

/// What one request the provider was sent carried.
#[derive(Debug, Clone)]
pub(crate) struct Sent {
    pub(crate) tools: Vec<String>,
    pub(crate) hosted: Vec<&'static str>,
    pub(crate) instructions: String,
    pub(crate) messages: Vec<Message>,
}

/// A provider answering each turn from a script, keeping every request.
#[derive(Debug, Default)]
pub(crate) struct Script {
    turns: Mutex<VecDeque<Vec<afr_providers::Result<Chunk>>>>,
    pub(crate) sent: Mutex<Vec<Sent>>,
}

impl Script {
    /// A provider whose turns are `turns`, in order.
    pub(crate) fn new(turns: impl IntoIterator<Item = Vec<Chunk>>) -> Arc<Self> {
        let turns = turns
            .into_iter()
            .map(|turn| turn.into_iter().map(Ok).collect())
            .collect();
        Arc::new(Self {
            turns: Mutex::new(turns),
            sent: Mutex::default(),
        })
    }

    /// A provider whose one turn streams `chunks`, then fails with `failure`.
    pub(crate) fn failing(chunks: Vec<Chunk>, failure: afr_providers::Error) -> Arc<Self> {
        let turn = chunks.into_iter().map(Ok).chain([Err(failure)]).collect();
        Arc::new(Self {
            turns: Mutex::new(VecDeque::from([turn])),
            sent: Mutex::default(),
        })
    }

    /// Every request sent so far.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        self.sent.lock().expect(UNPOISONED).clone()
    }
}

/// The script, shared between the loop and the test that reads it after.
#[derive(Debug)]
pub(crate) struct Shared(pub(crate) Arc<Script>);

impl Provider for Shared {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        self.0.sent.lock().expect(UNPOISONED).push(Sent {
            tools: request
                .tools
                .iter()
                .map(|spec| spec.name.to_owned())
                .collect(),
            hosted: request.hosted.iter().map(|entry| entry.name()).collect(),
            instructions: request.instructions.to_owned(),
            messages: request.messages.to_vec(),
        });
        let turn = self.0.turns.lock().expect(UNPOISONED).pop_front();
        futures_util::stream::iter(turn.unwrap_or_default()).boxed()
    }
}

/// A call to `name` with `arguments`, under provider id `id`.
pub(crate) fn call(id: &str, name: &str, arguments: serde_json::Value) -> Chunk {
    Chunk::Call(Call {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    })
}

/// Answer text.
pub(crate) fn say(text: &str) -> Chunk {
    Chunk::Text {
        kind: StreamTextKind::Answer,
        text: text.to_owned(),
    }
}

/// What a turn spent.
pub(crate) fn spent(input: u64, cached_input: u64, output: u64) -> Chunk {
    Chunk::Usage(Usage {
        input,
        cached_input,
        output,
    })
}

/// The schema a fixture tool offers: its name, and any object.
fn schema(entry: &'static Entry) -> Schema {
    Schema {
        description: entry.name(),
        parameters: serde_json::json!({"type": "object"}),
    }
}

/// A tool answering every call with `output`, or never answering when
/// `output` is empty.
#[derive(Debug)]
pub(crate) struct Canned {
    entry: &'static Entry,
    schema: Schema,
    output: String,
}

impl Canned {
    pub(crate) fn boxed(entry: &'static Entry, output: &str) -> Box<dyn Tool> {
        Box::new(Self {
            entry,
            schema: schema(entry),
            output: output.to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl Tool for Canned {
    fn entry(&self) -> &'static Entry {
        self.entry
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(&self, _arguments: &serde_json::Value, _context: ToolContext<'_>) -> ToolOutput {
        if self.output.is_empty() {
            return std::future::pending().await;
        }
        ToolOutput::succeeded(self.output.clone())
    }
}

/// A tool whose every call ran a process that exited with `code`.
#[derive(Debug)]
pub(crate) struct Exits {
    entry: &'static Entry,
    schema: Schema,
    code: i32,
}

impl Exits {
    pub(crate) fn boxed(entry: &'static Entry, code: i32) -> Box<dyn Tool> {
        Box::new(Self {
            entry,
            schema: schema(entry),
            code,
        })
    }
}

#[async_trait::async_trait]
impl Tool for Exits {
    fn entry(&self) -> &'static Entry {
        self.entry
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(&self, _arguments: &serde_json::Value, _context: ToolContext<'_>) -> ToolOutput {
        ToolOutput {
            text: format!("exited {}", self.code),
            exit_code: Some(self.code),
            error_code: None,
        }
    }
}

/// Every frame a run emits, in order.
#[derive(Debug, Default)]
pub(crate) struct Frames(pub(crate) Mutex<Vec<ActivityFrame<'static>>>);

impl Frames {
    pub(crate) fn emit(&self, frame: ActivityFrame<'static>) {
        self.0.lock().expect(UNPOISONED).push(frame);
    }

    pub(crate) fn taken(&self) -> Vec<ActivityFrame<'static>> {
        std::mem::take(&mut *self.0.lock().expect(UNPOISONED))
    }
}
