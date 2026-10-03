//! What the loop's suites share: a lease, a scripted provider that keeps every
//! request it was sent, and tools whose output or stall a test chooses.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::sync::mpsc;

use afd_wire::activity::ActivityFrame;
use afd_wire::lease::LeasePayload;
use afr_tools::{Entry, Schema, Tool, ToolContext, ToolOutput};

pub(crate) use self::model::{Script, Unreachable, call, ended, say, spent};
use crate::scrub::{Clean, Scrub};

#[path = "fixture/model.rs"]
mod model;

/// Why a fixture send cannot fail: the test holds the receiver until it reads.
pub(crate) const RECEIVER_HELD: &str = "the test holds the receiver for as long as the run";
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

    async fn call(&self, _arguments: &serde_json::Value, _context: ToolContext<'_, '_>) -> ToolOutput {
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

    async fn call(&self, _arguments: &serde_json::Value, _context: ToolContext<'_, '_>) -> ToolOutput {
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
