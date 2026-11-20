//! A report's proposal the daemon's own check refuses: a base that moved, or a
//! Definition the shared parser does not accept.
//!
//! The runner applied the same rules before it reported, so a refusal here
//! means the draft moved under the run or the two builds disagree. Neither is
//! the person's fault, and neither costs them the answer: the turn completes
//! with its text, writes no candidate, and keeps the refusal as its failure
//! detail. The refusal is logged once, under its registry code, naming the
//! draft and the run and never the Definition, which is the creator's private
//! text.

use af_core::{
    error_code::{self, ErrorCode},
    id::Uuid7,
};
use af_fleet_runtime::Definition;
use af_wire::run::SetupProposal;

use crate::Result;

/// Logged once per refused proposal, at `warn`, with `error_code`, `draft_id`
/// and `run_id`.
pub(super) const EVENT_PROPOSAL_REFUSED: &str = "setup_proposal_refused";

/// The failure detail a proposal on a moved base keeps: the person's next step
/// is to ask again, on the setup as it stands now.
pub(super) const DETAIL_BASE_MOVED: &str = "The setup changed while this answer was \
     written, so its proposed edit was not applied. Ask again to edit the current setup.";

/// The draft head a report's proposal must stand on.
#[derive(Debug, Clone, Copy)]
pub(super) struct Head<'a> {
    /// The head candidate, absent before the draft's first setup.
    pub(super) candidate: Option<&'a Uuid7>,
    /// The head candidate's revision; zero before the first.
    pub(super) revision: i64,
}

/// Why the daemon's check refused a report's proposal: the registry code the
/// refusal logs under, and the sentence the turn keeps as its failure detail.
#[derive(Debug)]
pub(super) struct Refusal {
    code: ErrorCode,
    detail: String,
}

impl Refusal {
    /// The sentence the turn keeps as its failure detail.
    pub(super) fn detail(&self) -> &str {
        &self.detail
    }

    /// Logs the refusal once, naming `draft` and `run`.
    pub(super) fn log(&self, draft: &Uuid7, run: &Uuid7) {
        let (error_code, draft_id, run_id) = (self.code.as_str(), draft.as_str(), run.as_str());
        tracing::warn!(event = EVENT_PROPOSAL_REFUSED, error_code, draft_id, run_id);
    }
}

impl From<af_fleet_runtime::Error> for Refusal {
    /// The parser's own code, and its sentence with each cause: the field the
    /// model got wrong, which is what the person reads to rephrase.
    fn from(error: af_fleet_runtime::Error) -> Self {
        Self {
            code: error.code(),
            detail: error.told(),
        }
    }
}

/// Checks `proposal` against the draft's `head` and parses its Definition
/// through the one parser every candidate passes: the Definition to append,
/// or why it was refused.
///
/// # Errors
/// A [`Refusal`] under `UZ-AUTHOR-003` when the base candidate or revision is
/// no longer the head, and under the parser's code when the Definition does
/// not parse or, for a first setup, breaks the first generation's bound.
pub(super) fn check(head: Head<'_>, proposal: &SetupProposal<'_>) -> Result<Definition, Refusal> {
    let revision = u64::try_from(head.revision).unwrap_or(u64::MAX);
    if head.candidate.map(Uuid7::as_str) != proposal.base_candidate_id.as_deref()
        || revision != proposal.base_revision
    {
        return Err(Refusal {
            code: error_code::AUTHORING_STALE,
            detail: DETAIL_BASE_MOVED.to_owned(),
        });
    }
    let definition = Definition::parse(&proposal.definition)?;
    if head.candidate.is_none() {
        definition.validate_initial_generation()?;
    }
    Ok(definition)
}

#[cfg(test)]
mod tests;
