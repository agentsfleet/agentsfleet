//! What the loop's suites share: a lease, a scripted provider that keeps every
//! request it was sent, and tools whose output or stall a test chooses.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use afd_wire::activity::{ActivityFrame, StreamTextKind};
use afd_wire::lease::LeasePayload;
use afr_providers::{Call, Chunk, Message, Provider, Request, Usage};
use afr_tools::{Entry, Schema, Tool, ToolContext, ToolOutput};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

use crate::scrub::{Clean, Scrub};

/// Why a fixture send cannot fail: the test holds the receiver until it reads.
const RECEIVER_HELD: &str = "the test holds the receiver for as long as the run";
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

/// The scrub of a lease carrying the fixture secrets.
pub(crate) fn scrub() -> Scrub {
    Scrub::new(&lease(&[], unbounded()).policy).unwrap()
}

/// `text` as the scrub hands it on.
pub(crate) fn clean(text: &str) -> Clean<String> {
    scrub().clean(text.to_owned())
}

/// `value` as the scrub hands it on.
pub(crate) fn clean_json(value: serde_json::Value) -> Clean<serde_json::Value> {
    scrub().clean_json(value)
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

/// One scripted turn: the chunks it streams, then the failure it ends on.
#[derive(Debug)]
struct Turn {
    chunks: Vec<Chunk>,
    failure: Option<fn() -> afr_providers::Error>,
}

impl Turn {
    fn replay(&self) -> Vec<afr_providers::Result<Chunk>> {
        let chunks = self.chunks.iter().cloned().map(Ok);
        chunks
            .chain(self.failure.map(|failure| Err(failure())))
            .collect()
    }
}

/// The turns a script plays, in order, read lock-free through a cursor.
#[derive(Debug)]
struct Turns {
    turns: Vec<Turn>,
    next: AtomicUsize,
}

/// A scripted model the test drives and reads back. The loop gets a
/// [`Replay`]; the requests it sends come back over a channel.
#[derive(Debug)]
pub(crate) struct Script {
    replay: Replay,
    received: mpsc::Receiver<Sent>,
    seen: RefCell<Vec<Sent>>,
}

impl Script {
    /// A model whose turns are `turns`, in order.
    pub(crate) fn new(turns: impl IntoIterator<Item = Vec<Chunk>>) -> Self {
        let turns = turns.into_iter().map(|chunks| Turn {
            chunks,
            failure: None,
        });
        Self::playing(turns.collect())
    }

    /// A model whose one turn streams `chunks`, then fails with `failure()`.
    pub(crate) fn failing(chunks: Vec<Chunk>, failure: fn() -> afr_providers::Error) -> Self {
        Self::playing(vec![Turn {
            chunks,
            failure: Some(failure),
        }])
    }

    fn playing(turns: Vec<Turn>) -> Self {
        let (sent, received) = mpsc::channel();
        let turns = Arc::new(Turns {
            turns,
            next: AtomicUsize::new(0),
        });
        Self {
            replay: Replay { turns, sent },
            received,
            seen: RefCell::default(),
        }
    }

    /// The provider the loop drives; every one plays the same turns.
    pub(crate) fn replay(&self) -> Replay {
        self.replay.clone()
    }

    /// Every request sent so far.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        self.seen.borrow_mut().extend(self.received.try_iter());
        self.seen.borrow().clone()
    }
}

/// The provider side of a [`Script`].
#[derive(Debug, Clone)]
pub(crate) struct Replay {
    turns: Arc<Turns>,
    sent: mpsc::Sender<Sent>,
}

impl Provider for Replay {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let sent = Sent {
            tools: request
                .tools
                .iter()
                .map(|spec| spec.name.to_owned())
                .collect(),
            hosted: request.hosted.iter().map(|entry| entry.name()).collect(),
            instructions: request.instructions.to_owned(),
            messages: request.messages.to_vec(),
        };
        self.sent.send(sent).expect(RECEIVER_HELD);
        let index = self.turns.next.fetch_add(1, Ordering::Relaxed);
        let turn = self.turns.turns.get(index).map(Turn::replay);
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

/// Every frame a run emits, in order, over the channel the supervisor's own
/// sink is.
#[derive(Debug)]
pub(crate) struct Frames {
    sent: mpsc::Sender<ActivityFrame<'static>>,
    received: mpsc::Receiver<ActivityFrame<'static>>,
}

impl Default for Frames {
    fn default() -> Self {
        let (sent, received) = mpsc::channel();
        Self { sent, received }
    }
}

impl Frames {
    /// The sink a run emits into.
    pub(crate) fn sink(&self) -> impl Fn(ActivityFrame<'static>) + Send + Sync + use<> {
        let sent = self.sent.clone();
        move |frame| sent.send(frame).expect(RECEIVER_HELD)
    }

    /// Every frame emitted since the last read.
    pub(crate) fn taken(&self) -> Vec<ActivityFrame<'static>> {
        self.received.try_iter().collect()
    }
}

/// A frame emitted and never read fails the test, the way Exonum's
/// `GuardedQueue` fails on a message nobody asserted: a duplicated or stray
/// end frame cannot pass unseen.
impl Drop for Frames {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let unread = self.taken();
        assert!(
            unread.is_empty(),
            "frames emitted and never read: {unread:?}"
        );
    }
}
