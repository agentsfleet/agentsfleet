//! The agent harness: the order one loop's turns run in, and nothing else.
//!
//! ```text
//!  embedder: agentsfleet-runner, a test, a local command
//!    │ implements Conversation: input, the window, the model call, the tools
//!    │ installs an Extension: what a run of its kind adds, or Plain
//!    ▼
//!  drive(conversation, extension) ──► Ending: Answered | Failed | Stopped
//!    every turn:  take input ─► before_model ─► prepare ─► sample ─► after_turn
//!    no calls:    on_answer ─► take input again: more input runs another turn
//!    calls:       run them ─► remember ─► after_group
//! ```
//!
//! The harness knows no lease, fleet or task. Input reaches the model at one
//! place, the top of every turn, and once more after an answer: a message that
//! arrives while the model writes its answer is read before the loop ends,
//! the way Codex continues a turn while input is pending. A model call in
//! flight is never cut short for new input; the call after it reads it.
//!
//! A message is text and media ([`input`]). Media a model cannot take reaches
//! it through the embedder's [`input::MediaReader`]: a voice note as its
//! transcript, a document as its text, or a note naming the part when the
//! embedder reads no such kind ([`input::Unread`]).
//!
//! Whatever a run of one kind adds, such as a durable task's input claims and
//! goal assessment, is an [`Extension`]. A run of no special kind installs
//! [`Plain`], so no branch in the loop asks what kind of run it is.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod conversation;
mod drive;
mod extension;
pub mod input;
mod turn;

pub use self::conversation::{Conversation, Prepared, Sampled};
pub use self::drive::{ANSWER_JOIN, drive};
pub use self::extension::{Answer, Budget, Extension, Plain};
pub use self::turn::{Ending, ModelTurn};

/// The README's examples, compiled and run as doctests by the unit lane, so
/// the crate's page cannot drift from its code.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
