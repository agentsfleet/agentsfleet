//! An agent harness: the order one loop's steps run in, the window each model
//! call sends, the transcript the loop keeps, the tools it offers the model,
//! and the children a run starts.
//!
//! ```text
//!  embedder: a service, a command-line tool, a test
//!    │ implements Model: one step streamed as chunks
//!    │ implements Conversation: input, the window, the request, what a call is lent, the stop
//!    │ implements Observe: each model call and tool call as it starts and ends
//!    │ installs an Extension: what a run of its kind adds, or Plain
//!    │ implements Raise: the loop each child of the run runs, polled beside the root
//!    ▼
//!  drive(conversation, extension, state) ──► Ending: Answered | Failed | Stopped | Waiting
//!    every step:  take input ─► before_model ─► window ─► model call ─► after_step
//!    no calls:    on_answer ─► remember ─► take input again: more input runs another step
//!    calls:       run them ─► remember ─► after_group
//!    a question:  run the step's calls ─► remember ─► quiet ─► Waiting
//! ```
//!
//! The harness decides when each step runs and when the loop ends. What a run
//! is for, where its tools run and what the model may read stay with the
//! embedder. Input reaches the model at one place, the top of every step, and
//! once more after an answer: a message that arrives while the model writes
//! its answer is read before the loop ends, the way Codex continues a step
//! while input is pending. A model call in flight is never cut short for new
//! input; the call after it reads it.
//!
//! The embedder fits each model call's [`Window`] before the call is made, the
//! way pi runs `transformContext`: the whole transcript, or a copy compacted to
//! fit ([`Conversation::window`]). The loop keeps every message in the
//! [`Transcript`], so fitting a window never loses one.
//!
//! The loop makes each model call itself: it sends the embedder's
//! [`Request`] to its [`Model`], reads the stream into a [`Step`] while the
//! embedder hears it ([`Listen`]), runs the step's calls and keeps the step
//! and its results in the [`Transcript`], scrubbed by the embedder first. What
//! a provider needs back to continue its reasoning rides each step as an
//! opaque [`Replay`].
//!
//! The embedder's [`Observe`]r hears each model call and each tool call start
//! and end, and the loop's own end. The loop opens, runs and ends every tool
//! call itself, so each ends once: a call the loop's stop cuts short, or whose
//! future is dropped, ends [`CallOutcome::Interrupted`].
//!
//! A message is text and media ([`input`]). Media a model cannot take reaches
//! it through the embedder's [`input::MediaReader`]: a voice note as its
//! transcript, a document as its text, or a note naming the part when the
//! embedder reads no such kind ([`input::Unread`]).
//!
//! Whatever a run of one kind adds, such as a long task's new inputs, its time
//! budget and its goal checks, is an [`Extension`]. A run of no special kind
//! installs [`Plain`], so no branch in the loop asks what kind of run it is.
//!
//! [`Describe`] is what the model is told of a tool, and [`Tool`] runs one
//! call with whatever the embedder lends that call: the state its calls
//! share, or the loop itself. [`Tools`] is the set one loop holds. A call to a
//! name it holds no tool for, and a call from a step cut at the output limit,
//! read the harness's own refusals ([`ToolOutput`]).
//!
//! A run's children are [`agents`]: loops a tool call starts, each the
//! embedder's own over a narrower set of its parent's tools. The root polls
//! them in its own future, so they end with it, and each ends once.
//!
//! A call that asks the person a question ([`ToolOutput::asks`]) ends the
//! loop [`Ending::Waiting`] once its step's other calls ran and the
//! conversation is quiet ([`Conversation::quiet`]), such as every child the
//! run started having ended. Nothing waits on the person inside the run, and
//! [`DriveState::resume`] answers the question later.
//!
//! The `tools` feature adds `tools`: commands, files, patches, the web,
//! memory, MCP servers, questions, deferred tools and the clock, each run
//! through a seam the embedder fills (`tools::Env`, `tools::Outbound`,
//! `tools::MemoryStore`), with host-process defaults for a standalone
//! harness.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod agents;
mod conversation;
mod drive;
mod extension;
pub mod input;
mod model;
mod observe;
mod step;
mod tool;
#[cfg(feature = "tools")]
pub mod tools;
mod transcript;
mod window;

pub use self::conversation::Conversation;
pub use self::drive::{ANSWER_JOIN, DriveState, drive};
pub use self::extension::{Answer, Budget, Extension, Plain};
pub use self::model::{Chunk, End, Hosted, Model, Request, TextKind, Usage};
pub use self::observe::{CallEnd, CallOutcome, Observe, StepEnd, StepOutcome, Unobserved};
pub use self::step::{Ending, Listen, Step, Waiting};
pub use self::tool::{
    Describe, ErrorCode, Image, ImageKind, Tool, ToolFuture, ToolOutput, ToolSpec, Tools,
};
pub use self::transcript::{Call, CallRef, Message, Replay, Transcript};
pub use self::window::{Fitted, Window};

/// The README's examples, compiled and run as doctests by the unit lane, so
/// the crate's page cannot drift from its code. The page shows the `tools`
/// feature too, so its examples compile with every feature on, as the lane
/// builds them.
#[cfg(all(doctest, feature = "tools"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
