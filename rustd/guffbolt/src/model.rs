//! The model seam: what one turn asks, the chunks its stream sends back, and
//! the [`Model`] an embedder streams them from.
//!
//! The types are provider-neutral. Each provider maps them onto its own wire,
//! so the loop never knows which provider it drives. The one provider-owned
//! value the loop carries is a turn's [`Replay`], which it hands back
//! unopened.

use std::fmt;
use std::ops::AddAssign;

use futures_util::stream::BoxStream;

use crate::tool::ToolSpec;
use crate::transcript::{Call, Message, Replay};

/// The name the provider's own web search is called by.
const WEB_SEARCH: &str = "web_search";

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
    /// The conversation so far, as it fits the model's window.
    pub messages: &'a [Message],
    /// The functions the model may call.
    pub tools: &'a [ToolSpec<'a>],
    /// The provider-hosted tools offered, sent as the provider's own specs.
    pub hosted: &'a [Hosted],
    /// The key the provider files the conversation's prompt cache under, so
    /// every request sharing it shares a cache.
    pub cache_key: &'a str,
    /// The most the turn may write, further clipped by the model's own limit.
    pub max_output_tokens: Option<u32>,
    /// Opened once: a turn that fails after it began is never opened again,
    /// so a request that may have reached the provider is not repeated.
    pub single_attempt: bool,
}

/// Tokens one turn spent, or a run summed over its turns.
///
/// Each prompt token is counted once, in exactly one of five places: read
/// fresh, read from the provider's cache, or written to it under one of three
/// lifetimes, so an embedder can price each at its own rate. Reasoning is
/// inside `output`, once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Prompt tokens the provider read fresh, neither from its cache nor
    /// written to it.
    pub input: u64,
    /// Prompt tokens read from the provider's cache.
    pub cached_input: u64,
    /// Prompt tokens written to the provider's cache with no lifetime
    /// reported, never folded into either lifetime.
    pub cache_write: u64,
    /// Prompt tokens written to the provider's cache for five minutes.
    pub cache_write_5m: u64,
    /// Prompt tokens written to the provider's cache for one hour.
    pub cache_write_1h: u64,
    /// Completion tokens, reasoning included.
    pub output: u64,
}

impl Usage {
    /// Every token the turn spent: the whole prompt and the completion.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.prompt().saturating_add(self.output)
    }

    /// The whole prompt, fresh, read and written: what fills the context
    /// window.
    #[must_use]
    pub const fn prompt(self) -> u64 {
        self.input
            .saturating_add(self.cached_input)
            .saturating_add(self.cache_written())
    }

    /// Prompt tokens written to the provider's cache, whatever their lifetime.
    #[must_use]
    pub const fn cache_written(self) -> u64 {
        self.cache_write
            .saturating_add(self.cache_write_5m)
            .saturating_add(self.cache_write_1h)
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.input = self.input.saturating_add(rhs.input);
        self.cached_input = self.cached_input.saturating_add(rhs.cached_input);
        self.cache_write = self.cache_write.saturating_add(rhs.cache_write);
        self.cache_write_5m = self.cache_write_5m.saturating_add(rhs.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.saturating_add(rhs.cache_write_1h);
        self.output = self.output.saturating_add(rhs.output);
    }
}

/// Which text a streamed chunk carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKind {
    /// The answer, which the conversation keeps.
    Answer,
    /// The model's reasoning, which only a listener sees.
    Reasoning,
}

/// How a turn ended, once every chunk of it was sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct End {
    /// What the provider needs back on the next turn.
    pub replay: Replay,
    /// The turn stopped at its output limit: a call it made may have been
    /// cut mid-argument and must not run.
    pub cut: bool,
}

/// One piece of a streamed turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Text, as the answer or the model's reasoning.
    Text {
        /// Which of the two it is.
        kind: TextKind,
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

/// A model a loop drives: one turn streamed as chunks, failing with the
/// embedder's own error.
pub trait Model: Send + Sync + fmt::Debug {
    /// Why a turn, or a measure, failed.
    type Error;

    /// The input tokens `request` would cost, measured as the provider counts
    /// them, so an embedder can fit its window before it sends.
    ///
    /// # Errors
    /// The request cannot be encoded for the provider.
    fn measure(&self, request: Request<'_>) -> Result<u64, Self::Error>;

    /// Streams one turn. The stream ends when the turn does; an error ends it
    /// early, and the run with it.
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Self::Error>>;

    /// Whether a call's result may carry an image for the model to see.
    fn accepts_images(&self) -> bool;
}
