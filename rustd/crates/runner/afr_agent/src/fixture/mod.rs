//! What the loop's suites share: a run, a scripted provider that keeps every
//! request it was sent, and tools whose output or stall a test chooses.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::sync::mpsc;

use af_wire::activity::ActivityFrame;
use af_wire::run::RunPayload;
use afr_tools::{CatalogTool, Entry, ToolContext};
use guffbolt::tools::{NoArguments, Schema};
use guffbolt::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};

pub(crate) use self::model::{Script, Sent, Unreachable, call, ended, say, spent, spent_caching};
use af_secrets::{Clean, Scrub};

mod model;

/// Why a fixture send cannot fail: the test holds the receiver until it reads.
pub(crate) const RECEIVER_HELD: &str = "the test holds the receiver for as long as the run";
/// Where a fixture run's context knobs sit.
const CONTEXT_POINTER: &str = "/policy/context";
/// The instant the fixture run expires at and its event was made at.
const AT: &str = "1970-01-01T00:00:00.001Z";

/// The provider key every fixture run carries.
pub(crate) const API_KEY: &str = "sk-test-0123456789";
/// The static credential every fixture run carries.
pub(crate) const GITHUB_TOKEN: &str = "ghp_fixture_token_abcdef";

/// A run offering `tools`, with `budget` as its context knobs.
pub(crate) fn payload(tools: &[&str], budget: serde_json::Value) -> RunPayload<'static> {
    let mut document = serde_json::json!({
        "run_id": "run-1", "fencing_token": 7, "run_expires_at": AT,
        "event": {"turn_id": "event-1", "fleet_id": "fleet-1", "workspace_id": "ws-1",
            "actor": "operator-1", "event_type": "chat",
            "request_json": "{\"message\":\"triage the failed run\"}", "created_at": AT},
        "policy": {"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": tools,
            "secrets_map": {"github": {"token": GITHUB_TOKEN, "host": "api.github.com"}},
            "mintable": [], "provider": "anthropic", "api_key": API_KEY,
            "inference_host": "h", "base_url": null, "repository_binding": null,
            "http_origin_policies": [], "context": null},
        "instructions": "Read the run.", "history": [], "resume_hold": false, "inputs_waiting": true
    });
    *document.pointer_mut(CONTEXT_POINTER).unwrap() = budget;
    // Leaked so the borrowed payload lives as long as the test that reads it.
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).unwrap()
}

/// The scrub of a run carrying the fixture secrets.
pub(crate) fn scrub() -> Scrub {
    Scrub::new(&payload(&[], unbounded()).policy).unwrap()
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

/// A tool answering every call with `output`, or never answering when
/// `output` is empty.
#[derive(Debug)]
pub(crate) struct Canned {
    entry: &'static Entry,
    schema: Schema,
    output: String,
}

impl Canned {
    pub(crate) fn boxed(entry: &'static Entry, output: &str) -> Box<CatalogTool> {
        Box::new(Self {
            entry,
            schema: Schema::of::<NoArguments>(entry.name()),
            output: output.to_owned(),
        })
    }
}

impl Describe for Canned {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(self.entry.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Canned {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        _cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(async move {
            if self.output.is_empty() {
                return std::future::pending().await;
            }
            ToolOutput::succeeded(self.output.clone())
        })
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
    pub(crate) fn boxed(entry: &'static Entry, code: i32) -> Box<CatalogTool> {
        Box::new(Self {
            entry,
            schema: Schema::of::<NoArguments>(entry.name()),
            code,
        })
    }
}

impl Describe for Exits {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(self.entry.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Exits {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        _cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        let mut exited = ToolOutput::succeeded(format!("exited {}", self.code));
        exited.exit_code = Some(self.code);
        Box::pin(std::future::ready(exited))
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
