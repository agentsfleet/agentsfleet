//! The model's proposal, read against the head candidate the caller lends:
//! exact text anchors become native byte ranges, an edit naming any other
//! candidate is refused, and so is a first setup once a head exists.

use serde::Deserialize;

use super::{Refused, Rejection, verbatim};
use af_validate::TEXT_MAX_CHARS;

use crate::definition::{ConfigOp, Definition, EditBatch, InstructionEdit};

/// The bytes of a candidate identifier, a hyphenated UUID.
pub(super) const CANDIDATE_ID_BYTES: usize = 36;
/// The digits of a revision: a positive signed 64-bit database integer.
pub(super) const REVISION_DIGITS_MAX: usize = 19;

/// A suggested setup change, untrusted until the native editor accepts it.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Proposal<I> {
    /// A first candidate, parsed by the same Definition validator as the drawer.
    Initialize {
        /// The complete setup.
        #[garde(skip)]
        definition: Box<Definition>,
    },
    /// A small edit against the exact head candidate.
    Edit {
        /// The head candidate's identifier.
        #[garde(length(bytes, min = CANDIDATE_ID_BYTES, max = CANDIDATE_ID_BYTES))]
        base_candidate_id: String,
        /// The head candidate's decimal revision.
        #[garde(length(bytes, min = 1, max = REVISION_DIGITS_MAX), custom(af_validate::rules::ascii_digits))]
        base_candidate_revision: String,
        /// Configuration operations, validated by the native editor.
        #[garde(skip)]
        config_ops: Vec<ConfigOp>,
        /// Instruction edits, validated by the native editor.
        #[garde(skip)]
        instruction_edits: Vec<I>,
    },
}

/// A proposal as the model writes it: instruction edits as exact text.
pub type Suggestion = Proposal<TextEdit>;
/// A proposal as the native editor reads it: instruction edits as byte ranges.
pub type Native = Proposal<InstructionEdit>;

/// One instruction edit as the model writes it: text to add, or exact text
/// to replace, never an offset it would have to count.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextEdit {
    /// New text after the current instructions.
    Append {
        /// The text, verbatim.
        #[garde(length(chars, max = TEXT_MAX_CHARS), custom(af_validate::rules::nul_free))]
        text: String,
    },
    /// Exact text that occurs once, and what replaces it.
    Replace {
        /// The text to replace.
        #[garde(length(chars, max = TEXT_MAX_CHARS), custom(af_validate::rules::nul_free))]
        old_text: String,
        /// What replaces it.
        #[garde(length(chars, max = TEXT_MAX_CHARS), custom(af_validate::rules::nul_free))]
        text: String,
    },
}

/// The candidate a proposal must name, and the setup its anchors resolve in.
#[derive(Debug, Clone, Copy)]
pub struct Head<'a> {
    /// The candidate's identifier.
    pub id: &'a str,
    /// The candidate's revision.
    pub revision: u64,
    /// The setup an edit applies to.
    pub definition: &'a Definition,
}

impl Native {
    /// The whole setup this proposal leaves: a first setup as proposed, or
    /// `head` with the edit applied.
    ///
    /// # Errors
    /// The checker refuses the setup, with its own account of which rule; or
    /// an edit has no setup to apply to.
    pub fn apply(self, head: Option<&Definition>) -> Result<Definition, Refused> {
        match self {
            Self::Initialize { definition } => Ok(*definition),
            Self::Edit {
                config_ops,
                instruction_edits,
                ..
            } => head
                .ok_or(Refused {
                    reason: Rejection::StaleCandidate,
                    detail: None,
                })?
                .edit(EditBatch {
                    config_ops,
                    instruction_edits,
                })
                .map_err(Refused::checked),
        }
    }
}

/// `proposal` with its text anchors resolved against `head`, whose identity
/// an edit must name exactly.
pub(super) fn native(
    proposal: Suggestion,
    head: Option<&Head<'_>>,
    said: Option<&str>,
) -> Result<Native, Rejection> {
    let proposal = garde::Unvalidated::new(proposal)
        .validate()
        .map_err(|_untrusted| Rejection::InvalidArguments)?
        .into_inner();
    match proposal {
        // A first setup is a draft's first: once the draft has a head, only an
        // edit naming it may change the setup.
        Proposal::Initialize { .. } if head.is_some() => Err(Rejection::StaleCandidate),
        Proposal::Initialize { definition } => Ok(Proposal::Initialize { definition }),
        Proposal::Edit {
            base_candidate_id,
            base_candidate_revision,
            config_ops,
            instruction_edits,
        } => {
            // Compared as a number, so a spelling of the head's revision with
            // leading zeros names the head rather than a stale candidate.
            let revision: u64 = base_candidate_revision
                .parse()
                .map_err(|_untrusted| Rejection::InvalidArguments)?;
            let base = head
                .filter(|base| base.id == base_candidate_id && base.revision == revision)
                .ok_or(Rejection::StaleCandidate)?;
            let instruction_edits =
                instructions(base.definition.instructions_md(), instruction_edits, said)?;
            Ok(Proposal::Edit {
                base_candidate_id,
                base_candidate_revision,
                config_ops,
                instruction_edits,
            })
        }
    }
}

fn instructions(
    base: &str,
    edits: Vec<TextEdit>,
    said: Option<&str>,
) -> Result<Vec<InstructionEdit>, Rejection> {
    let mut ranges = Vec::new();
    let mut appended = None::<String>;
    for edit in edits {
        let edit = garde::Unvalidated::new(edit)
            .validate()
            .map_err(|_untrusted| Rejection::InvalidArguments)?;
        match edit.into_inner() {
            // Inserted text is the person's own where the model retyped it.
            TextEdit::Append { text } => appended
                .get_or_insert_default()
                .push_str(&verbatim::restore(&text, said)),
            TextEdit::Replace { old_text, text } => {
                ranges.push(replace(base, &old_text, verbatim::restore(&text, said))?);
            }
        }
    }
    // All additions share one base position. Joining them preserves their
    // declared order without producing overlapping native insertions.
    if let Some(text) = appended {
        ranges.push(InstructionEdit {
            start_byte: base.len(),
            end_byte: base.len(),
            text,
        });
    }
    Ok(ranges)
}

fn replace(base: &str, old_text: &str, text: String) -> Result<InstructionEdit, Rejection> {
    let start_byte = base.find(old_text).ok_or(Rejection::InstructionsMissing)?;
    let suffix = base
        .get(start_byte..)
        .ok_or(Rejection::InstructionsMissing)?;
    // Search after one character rather than after the entire match: anchors
    // can overlap (for example, "aa" occurs twice in "aaa").
    if suffix
        .chars()
        .next()
        .and_then(|first| suffix.get(first.len_utf8()..))
        .is_some_and(|tail| tail.contains(old_text))
    {
        return Err(Rejection::InstructionsAmbiguous);
    }
    // String matches prove UTF-8 boundaries. The native editor still checks
    // overlap, the complete replacement bound and the final setup atomically.
    Ok(InstructionEdit {
        start_byte,
        end_byte: start_byte + old_text.len(),
        text,
    })
}

#[cfg(test)]
#[path = "tests/text_edits.rs"]
mod tests;
