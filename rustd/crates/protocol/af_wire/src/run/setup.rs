//! A setup draft's turn on the common run path: who owns the run, what the
//! assignment carries for the draft, and what its report hands back.
//!
//! A draft-owned run is a [`super::RunPayload`] like a fleet's. Its
//! `owner` names the draft and carries its [`SetupWork`], its `event` is the draft's queued message, its
//! `history` the draft's earlier turns, and its policy offers one tool,
//! `propose_setup`, with no sandbox. The runner applies proposals in memory
//! and reports the final setup once; the daemon re-checks it in the report
//! transaction and writes at most one candidate. The draft's compacted
//! context rides the fenced context verb (`crate::context`), not the payload.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The longest setup Definition a report may carry, as JSON text: the bound
/// the Definition parser applies, so a report never refuses what it admits.
pub const SETUP_DEFINITION_BYTES_MAX: usize = af_validate::DEFINITION_MAX_BYTES;
/// The bytes of a candidate identifier, a hyphenated UUID.
const CANDIDATE_ID_BYTES: usize = 36;

/// Whose work a run runs, and what that owner's work carries beside the
/// common payload.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = RunOwnerRef))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Owner<'a> {
    /// An installed fleet's event.
    Fleet {
        /// The fleet.
        #[serde(borrow)]
        fleet_id: Cow<'a, str>,
    },
    /// A setup draft's queued message.
    Draft {
        /// The draft.
        #[serde(borrow)]
        draft_id: Cow<'a, str>,
        /// The draft's setup turn.
        #[serde(borrow)]
        setup: Box<SetupWork<'a>>,
    },
}

impl<'a> Owner<'a> {
    /// The draft's setup turn; `None` on a fleet's run.
    #[must_use]
    pub fn setup(&self) -> Option<&SetupWork<'a>> {
        match self {
            Self::Fleet { .. } => None,
            Self::Draft { setup, .. } => Some(setup.as_ref()),
        }
    }
}

/// The setup a draft's turn starts from.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupCandidate<'a> {
    /// The head candidate's identifier: a report's base names it.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// The head candidate's revision: a report's base names it.
    pub revision: u64,
    /// The head candidate's Definition.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub definition: serde_json::Value,
}

/// What a draft-owned assignment carries beside its message and history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupWork<'a> {
    /// The message's position in the draft's conversation.
    pub sequence: u64,
    /// The draft's head candidate, absent before its first setup.
    #[serde(borrow)]
    pub candidate: Option<SetupCandidate<'a>>,
    /// The person asked to save with this message: the turn proposes nothing,
    /// and the report saves the head candidate.
    pub save_requested: bool,
    /// The `propose_setup` tool's definition: its description and parameters.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub tool: serde_json::Value,
    /// Each [`super::RunPayload::history`] turn's position in the draft's
    /// conversation, oldest first and in step with it: the sequence a saved
    /// context's watermark counts in. A run skips the turns a loaded context
    /// covers, and compacts only turns it can name. Empty from a daemon that
    /// sends none: the run then neither skips nor compacts a turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history_sequences: Vec<u64>,
}

/// The setup a turn proposed, as one final Definition on one base.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SetupProposal<'a> {
    /// The candidate the proposal edits, absent for a first setup.
    #[serde(borrow)]
    #[garde(inner(length(bytes, min = CANDIDATE_ID_BYTES, max = CANDIDATE_ID_BYTES)))]
    pub base_candidate_id: Option<Cow<'a, str>>,
    /// That candidate's revision; zero for a first setup.
    #[garde(skip)]
    pub base_revision: u64,
    /// The final Definition, as JSON text the daemon parses.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = SETUP_DEFINITION_BYTES_MAX), custom(af_validate::rules::nul_free))]
    pub definition: Cow<'a, str>,
}

/// A draft-owned run's report payload. Present only on such a run's
/// report, which is how the daemon tells the two apart.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SetupReport<'a> {
    /// The setup the turn proposed, absent when it proposed none.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub proposal: Option<SetupProposal<'a>>,
}
