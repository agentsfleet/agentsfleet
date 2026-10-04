//! The provider seam: one model turn, streamed as chunks.
//!
//! The types here are provider-neutral. Each wire maps them onto its own
//! function-calling shape, so the loop never knows which provider it drives.
//! The one provider-owned value the loop carries is a turn's [`Replay`], which
//! it hands back untouched.

use std::fmt;
use std::ops::AddAssign;

use afd_wire::activity::StreamTextKind;
use futures_util::stream::BoxStream;
use rig_core::message::AssistantContent;

use crate::error::Result;

/// The name the provider's own web search is called by.
const WEB_SEARCH: &str = "web_search";

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The provider's id for the call, echoed with its result.
    pub id: String,
    /// The tool's name.
    pub name: String,
    /// The arguments, as the model wrote them.
    pub arguments: serde_json::Value,
}

/// What a provider needs back on the next turn to continue its own reasoning.
///
/// Thinking blocks with their signatures, `reasoning_content`, or a gateway's
/// reasoning details. Opaque to the loop, which keeps it with the turn that
/// produced it and hands it back unopened; a provider that reasons in the
/// open, or not at all, leaves it empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Replay(pub(crate) Vec<AssistantContent>);

impl Replay {
    /// Whether there is nothing to hand back.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// How a turn ended, once every chunk of it was sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct End {
    /// What the provider needs back on the next turn.
    pub replay: Replay,
    /// The turn stopped at its output limit: a call it made may have been
    /// cut mid-argument and must not run.
    pub cut: bool,
}

/// One message of the conversation a turn continues.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// What the fleet was asked.
    User(String),
    /// What the model answered and which tools it called.
    Assistant {
        /// The answer text of the turn, possibly empty.
        text: String,
        /// The calls the turn asked for.
        calls: Vec<Call>,
        /// What the provider needs back to continue its own reasoning.
        replay: Replay,
    },
    /// One call's output, fed back to the model.
    ToolResult {
        /// The provider's id for the call.
        call_id: String,
        /// What the call returned.
        output: String,
    },
}

/// One tool as the model is offered it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSpec<'a> {
    /// The name the model calls it by.
    pub name: &'a str,
    /// What the tool does.
    pub description: &'a str,
    /// The arguments' JSON Schema.
    pub parameters: &'a serde_json::Value,
}

/// A tool the provider runs itself, offered as the wire's own spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosted {
    /// The provider's web search.
    WebSearch,
}

impl Hosted {
    /// The name the model calls it by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::WebSearch => WEB_SEARCH,
        }
    }
}

/// What one turn asks the model.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The model to run.
    pub model: &'a str,
    /// The system prompt.
    pub instructions: &'a str,
    /// The conversation so far.
    pub messages: &'a [Message],
    /// The functions the model may call; empty once the context cap is reached.
    pub tools: &'a [ToolSpec<'a>],
    /// The provider-hosted tools offered, sent as the provider's own specs.
    pub hosted: &'a [Hosted],
}

/// Tokens one turn spent, or a run summed over its turns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Prompt tokens, cached ones included.
    pub input: u64,
    /// Prompt tokens read from the provider's cache.
    pub cached_input: u64,
    /// Completion tokens.
    pub output: u64,
}

impl Usage {
    /// Prompt and completion tokens together.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.input.saturating_add(self.output)
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.input = self.input.saturating_add(rhs.input);
        self.cached_input = self.cached_input.saturating_add(rhs.cached_input);
        self.output = self.output.saturating_add(rhs.output);
    }
}

/// One piece of a streamed turn.
#[derive(Debug, Clone, PartialEq)]
pub enum Chunk {
    /// Text, as the answer or the model's reasoning.
    Text {
        /// Which of the two it is.
        kind: StreamTextKind,
        /// The text.
        text: String,
    },
    /// A complete tool call.
    Call(Call),
    /// What the turn spent.
    Usage(Usage),
    /// How the turn ended; the last chunk of a turn that ended.
    End(End),
}

/// A model provider.
pub trait Provider: Send + Sync + fmt::Debug {
    /// Streams one turn. The stream ends when the turn does; an error ends it
    /// early, and the run with it.
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>>;
}
