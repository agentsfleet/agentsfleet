//! The setup proposal tool's rules: what the model is told, how its call is
//! read, and what it hears when a proposal is refused.
//!
//! ```text
//!  arguments ─► settle ─► Suggestion ─► anchors resolved on Head ─► Native ─► apply ─► Definition
//!     (omissions mean none)   │        (edit must name the head)       │   (checker: edit or
//!                             │                                        │    first-setup bound)
//!                             └──────────── refused: Rejection + the checker's account
//! ```
//!
//! One copy, read wherever a setup proposal is checked, so a refused proposal
//! reads the same to the model on every path. It holds no candidate: the
//! caller lends the head as a [`Head`], and the person's newest message, whose
//! wording an inserted instruction keeps.

mod proposal;
mod rejection;
mod schema;
mod verbatim;

use serde::Deserialize;
use serde_json::Value;

use crate::{Definition, Error};

pub use self::proposal::{Head, Native, Proposal, Suggestion, TextEdit};
pub use self::rejection::{Rejection, invalid_setup, stale_base};
pub use self::schema::{parameters, settle};

/// The tool's name, as the model calls it.
pub const NAME: &str = "propose_setup";

/// What the tool does, as the model reads it.
pub const DESCRIPTION: &str = "Call this tool only when the person's newest message asks to create or change the setup. \
    Do not call it for a question, a greeting or an approval such as Save, Keep it or OK; answer those in plain text. \
    Return a setup proposal for native validation. This tool does not save or run a fleet. \
    Include only the settings the person asked for. When the fleet needs a fact the person has not given, \
    such as which repository to work on, ask them one question instead of calling this tool. \
    When they gave no budget, choose a small daily budget, say which in your reply, \
    and ask them to confirm or change it before they save.";

/// The tool's one argument, which carries the proposal object itself rather
/// than JSON inside a string.
pub const PROPOSAL_ARGUMENT: &str = "proposal";

/// A refused proposal: why, and the checker's own account when it named the
/// rule to fix.
#[derive(Debug)]
pub struct Refused {
    /// Why it was refused.
    pub reason: Rejection,
    /// The checker's refusal. Its sentence names keys and rules, never a
    /// value, so the model may read it; its field may be logged.
    pub detail: Option<Error>,
}

impl Refused {
    /// `reason` for `settled` arguments: unreadable arguments carry the
    /// checker's account of the first setup they held, when it refuses one.
    #[must_use]
    pub fn new(reason: Rejection, settled: &Value) -> Self {
        let detail = (reason == Rejection::InvalidArguments)
            .then(|| first_setup_failure(settled))
            .flatten();
        Self { reason, detail }
    }

    /// A setup the checker refuses, with its account of which rule.
    fn checked(failure: Error) -> Self {
        Self {
            reason: Rejection::InvalidArguments,
            detail: Some(failure),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal: Suggestion,
}

/// The model's tool arguments read as a proposal against `head`.
///
/// They are settled, parsed, and every text anchor is resolved in the head's
/// instructions. Inserted text keeps the wording of `said`, the person's
/// newest message, where the model retyped it.
///
/// # Errors
/// The arguments do not parse, break a bound, name another candidate, or an
/// anchor matches no text or more than one place.
pub fn read(
    arguments: Value,
    head: Option<&Head<'_>>,
    said: Option<&str>,
) -> Result<Native, Refused> {
    let arguments = settle(arguments);
    Arguments::deserialize(&arguments)
        // serde's message quotes the refused value, so it is dropped; the
        // checker's account below names keys and rules instead.
        .map_err(|_untrusted| Rejection::InvalidArguments)
        .and_then(|parsed| proposal::native(parsed.proposal, head, said))
        .map_err(|reason| Refused::new(reason, &arguments))
}

/// The checker's account of a first setup's definition, written as JSON
/// text, that it refuses.
#[must_use]
pub fn initial_refusal(definition: &str) -> Option<Error> {
    Definition::parse(definition).err()
}

/// The checker's own account of the first setup in `arguments` it refuses.
fn first_setup_failure(arguments: &Value) -> Option<Error> {
    let definition = arguments.get(PROPOSAL_ARGUMENT)?.get("definition")?;
    Definition::parse(&definition.to_string()).err()
}

#[cfg(test)]
#[path = "tests/initial.rs"]
mod tests;
