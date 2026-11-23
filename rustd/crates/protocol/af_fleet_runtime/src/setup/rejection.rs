//! Why a proposal was refused, and what the model hears so it can correct it.
//!
//! The texts never carry a refused value: the checker's own sentence names
//! keys and rules, and every other refusal is a fixed sentence.

/// A correctable model input failure, never a native activity or Rust error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Unsupported argument shape, a declared bound violated, or a setup the
    /// checker refuses.
    InvalidArguments,
    /// Exact source text does not occur in the head's instructions.
    InstructionsMissing,
    /// More than one source position matches the requested replacement.
    InstructionsAmbiguous,
    /// The edit does not name the head candidate.
    StaleCandidate,
    /// The newest message asks for no setup change.
    EditNotRequested,
}

impl Rejection {
    /// The refusal's stable name, which a log and the model's result carry.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::InvalidArguments => "invalid_arguments",
            Self::InstructionsMissing => "instructions_missing",
            Self::InstructionsAmbiguous => "instructions_ambiguous",
            Self::EditNotRequested => "edit_not_requested",
            Self::StaleCandidate => "stale_candidate",
        }
    }

    /// What the model hears when the checker named no setting to fix.
    #[must_use]
    pub const fn feedback(self) -> &'static str {
        match self {
            Self::InvalidArguments => {
                "No changes were applied. Correct the propose_setup arguments using the fixed schema. For initialize, every setting except name goes inside x-agentsfleet, settings the person did not ask for are omitted, and budget.daily_nanos is a decimal string. For edit, base_candidate_revision is a decimal string and instruction edits use kind=append or kind=replace, never byte offsets."
            }
            Self::InstructionsMissing => {
                "No changes were applied. The old_text did not match the current instructions_md. Copy a unique exact substring, including whitespace, without adding bullet markers. To add instructions, use kind=append with only the new text. Return a corrected propose_setup call."
            }
            Self::InstructionsAmbiguous => {
                "No changes were applied. The old_text matches more than one position in instructions_md. Include enough exact surrounding text to identify one position. Return a corrected propose_setup call."
            }
            Self::EditNotRequested => {
                "No changes were applied. The newest message does not ask to change the setup. Answer the question without proposing an edit."
            }
            Self::StaleCandidate => {
                "No changes were applied. Use the current candidate identifier and decimal revision from the current candidate or most recent accepted proposal receipt. Return a corrected propose_setup call."
            }
        }
    }
}

/// What the model hears when the checker named the rule to fix: `told` is
/// the checker's own sentence.
#[must_use]
pub fn invalid_setup(told: &str) -> String {
    format!(
        "No changes were applied. The setup is invalid: {told}. Fix that, keep only the settings the person asked for, and return a corrected propose_setup call."
    )
}

/// What the model hears when its edit names another candidate than the head
/// `id` at `revision`, which it is told so it can name them.
#[must_use]
pub fn stale_base(id: &str, revision: u64) -> String {
    format!(
        "No changes were applied. The current candidate is {id} at revision {revision}. Name that identifier and revision \"{revision}\" as the base and return a corrected propose_setup call."
    )
}
