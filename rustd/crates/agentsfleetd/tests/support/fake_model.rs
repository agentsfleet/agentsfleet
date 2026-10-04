//! A model behind the provider seam, for the bundle suite.
//!
//! The real loop (`afr_agent::Loop`) drives it exactly as it drives a hosted
//! provider. Each turn is decided by a script that reads the request — the
//! system prompt the daemon rendered and every tool output fed back so far —
//! so a script takes its repair branch from the trusted repair context and
//! branches on what an upstream answered, as a model following the SKILL.md
//! would. Every request is recorded besides, so a suite asserts what the model
//! was offered and what each call returned, a refusal's
//! `[ToolErrorCode] detail` included.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use afd_wire::activity::StreamTextKind;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;
use afr_providers::{Call, Chunk, Connect, End, Message, Provider, Request, Usage};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

/// Tokens every scripted turn reports, so the report's sums are non-zero.
const TURN_USAGE: Usage = Usage {
    input: 100,
    cached_input: 0,
    output: 10,
};

/// The line of the trusted repair context that names the branch.
const REPAIR_BRANCH: &str = "repair branch:";

/// What one request to the model carried.
#[derive(Debug, Clone)]
pub(crate) struct Asked {
    /// Which turn of the run this is, from 0.
    pub(crate) turn: usize,
    /// The tools offered, by name.
    pub(crate) tools: Vec<String>,
    /// The system prompt.
    pub(crate) instructions: String,
    /// Every tool output fed back so far, in order.
    pub(crate) results: Vec<String>,
}

/// Decides one turn's chunks from what the model was asked.
type Decide = dyn Fn(&Asked) -> Vec<Chunk> + Send + Sync;

struct Script {
    decide: Box<Decide>,
    next: AtomicUsize,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Script")
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}

/// The model a test scripts and reads back.
#[derive(Debug, Clone)]
pub(crate) struct FakeModel {
    script: Arc<Script>,
    asked: mpsc::Sender<Asked>,
}

/// The test's end of a [`FakeModel`].
#[derive(Debug)]
pub(crate) struct Transcript(mpsc::Receiver<Asked>);

impl Transcript {
    /// Every request the model received, in order.
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.0.try_iter().collect()
    }
}

impl FakeModel {
    /// A model playing `turns` in order, and the transcript of what it was sent.
    pub(crate) fn new(turns: Vec<Vec<Chunk>>) -> (Self, Transcript) {
        Self::deciding(move |asked| turns.get(asked.turn).cloned().unwrap_or_default())
    }

    /// A model whose every turn is `decide` over the request.
    pub(crate) fn deciding(
        decide: impl Fn(&Asked) -> Vec<Chunk> + Send + Sync + 'static,
    ) -> (Self, Transcript) {
        let (asked, transcript) = mpsc::channel();
        let script = Arc::new(Script {
            decide: Box::new(decide),
            next: AtomicUsize::new(0),
        });
        (Self { script, asked }, Transcript(transcript))
    }
}

impl Connect for FakeModel {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _lease: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl Provider for FakeModel {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let results = (request.messages.iter())
            .filter_map(|message| match message {
                Message::ToolResult { output, .. } => Some(output.clone()),
                Message::User(_) | Message::Assistant { .. } => None,
            })
            .collect();
        let asked = Asked {
            turn: self.script.next.fetch_add(1, Ordering::Relaxed),
            tools: request
                .tools
                .iter()
                .map(|spec| spec.name.to_owned())
                .collect(),
            instructions: request.instructions.to_owned(),
            results,
        };
        let turn = (self.script.decide)(&asked);
        // A suite that dropped its transcript asserts nothing about requests.
        let _unread = self.asked.send(asked);
        let ended = [Chunk::Usage(TURN_USAGE), Chunk::End(End::default())];
        futures_util::stream::iter(turn.into_iter().chain(ended).map(Ok)).boxed()
    }
}

/// An `http_request` call, under provider id `id`.
pub(crate) fn http(id: &str, arguments: serde_json::Value) -> Chunk {
    call(id, "http_request", arguments)
}

/// A call to `name` with `arguments`, under provider id `id`.
pub(crate) fn call(id: &str, name: &str, arguments: serde_json::Value) -> Chunk {
    Chunk::Call(Call {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    })
}

/// The repair branch the trusted repair context names, if the prompt has one.
pub(crate) fn repair_branch(asked: &Asked) -> Option<&str> {
    asked
        .instructions
        .lines()
        .find_map(|line| line.trim().strip_prefix(REPAIR_BRANCH))
        .map(str::trim)
}

/// Answer text.
pub(crate) fn say(text: &str) -> Chunk {
    Chunk::Text {
        kind: StreamTextKind::Answer,
        text: text.to_owned(),
    }
}
