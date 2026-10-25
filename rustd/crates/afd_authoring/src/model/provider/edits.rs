//! Resolve exact text anchors against the daemon's immutable candidate snapshot.

use crate::{Result, types::Candidate};
use afd_fleet_runtime::definition::{
    ConfigOp, Definition, InstructionEdit, MAX_INSTRUCTIONS_CHARS,
};
use afd_wire::authoring::{Proposal, ProposalRejection};
use serde::Deserialize;

pub(super) type Suggestion = Proposal<Definition, ConfigOp, TextEdit>;
type Native = Proposal<Definition, ConfigOp, InstructionEdit>;

#[derive(Deserialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TextEdit {
    Append {
        #[garde(length(chars, max = MAX_INSTRUCTIONS_CHARS), custom(afd_validate::rules::nul_free))]
        text: String,
    },
    Replace {
        #[garde(length(chars, max = MAX_INSTRUCTIONS_CHARS), custom(afd_validate::rules::nul_free))]
        old_text: String,
        #[garde(length(chars, max = MAX_INSTRUCTIONS_CHARS), custom(afd_validate::rules::nul_free))]
        text: String,
    },
}

pub(super) fn native(
    proposal: Suggestion,
    candidate: Option<&Candidate>,
) -> Result<Native, ProposalRejection> {
    let proposal = garde::Unvalidated::new(proposal)
        .validate()
        .map_err(|_untrusted| ProposalRejection::InvalidArguments)?
        .into_inner();
    match proposal {
        Proposal::Initialize { definition } => Ok(Proposal::Initialize { definition }),
        Proposal::Edit {
            base_candidate_id,
            base_candidate_revision,
            config_ops,
            instruction_edits,
        } => {
            let base = candidate
                .filter(|base| {
                    base.id.as_str() == base_candidate_id
                        && base.revision == base_candidate_revision
                })
                .ok_or(ProposalRejection::StaleCandidate)?;
            let instruction_edits =
                instructions(base.definition.instructions_md(), instruction_edits)?;
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
) -> Result<Vec<InstructionEdit>, ProposalRejection> {
    let mut ranges = Vec::new();
    let mut appended = None::<String>;
    for edit in edits {
        let edit = garde::Unvalidated::new(edit)
            .validate()
            .map_err(|_untrusted| ProposalRejection::InvalidArguments)?;
        match edit.into_inner() {
            TextEdit::Append { text } => appended.get_or_insert_default().push_str(&text),
            TextEdit::Replace { old_text, text } => ranges.push(replace(base, &old_text, text)?),
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

fn replace(base: &str, old_text: &str, text: String) -> Result<InstructionEdit, ProposalRejection> {
    let start_byte = base
        .find(old_text)
        .ok_or(ProposalRejection::InstructionsMissing)?;
    let suffix = base
        .get(start_byte..)
        .ok_or(ProposalRejection::InstructionsMissing)?;
    // Search after one character rather than after the entire match: anchors
    // can overlap (for example, "aa" occurs twice in "aaa").
    if suffix
        .chars()
        .next()
        .and_then(|first| suffix.get(first.len_utf8()..))
        .is_some_and(|tail| tail.contains(old_text))
    {
        return Err(ProposalRejection::InstructionsAmbiguous);
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
pub(super) mod tests;
