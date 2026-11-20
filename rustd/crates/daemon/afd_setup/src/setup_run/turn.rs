//! The turn an assignment took, as the assignment carries it: the message,
//! its model, the draft's head candidate and the earlier turns.

use af_core::id::Uuid7;
use afd_credential::provider::RegisteredModel;
use jiff::Timestamp;

use crate::{Result, types::Candidate};

/// One earlier turn of the draft's conversation. Its `Debug` hides both
/// texts: they are the creator's private conversation.
#[derive(Clone)]
pub struct SetupTurnText {
    /// The turn's position in the draft, which a saved context's watermark
    /// counts in.
    pub sequence: i64,
    /// What the person asked.
    pub message: String,
    /// What setup answered.
    pub answer: String,
}

/// The turn an assignment took, with everything the assignment carries. Its `Debug`
/// names the draft and message only.
pub struct SetupTurn {
    /// The draft, which is also its thread's id.
    pub draft_id: Uuid7,
    /// The draft's workspace.
    pub workspace_id: Uuid7,
    /// The tenant who pays.
    pub tenant_id: Uuid7,
    /// The draft's creator, who sent the message.
    pub user_id: Uuid7,
    /// The message the run runs: its `turn_id`.
    pub message_id: Uuid7,
    /// The message's position in the draft.
    pub sequence: i64,
    /// When the message was accepted.
    pub message_created_at: Timestamp,
    /// The person's text.
    pub user_text: String,
    /// The person asked to save with this message.
    pub save_requested: bool,
    /// The message was running already: its earlier run lapsed, or went with
    /// its runner's deletion. This assignment reassigns it.
    pub reassigned: bool,
    /// The model the message pinned, resolved and admitted.
    pub model: RegisteredModel,
    /// The draft's head candidate.
    pub candidate: Option<SetupHead>,
    /// Earlier answered turns, oldest first.
    pub history: Vec<SetupTurnText>,
}

impl std::fmt::Debug for SetupTurnText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupTurnText").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SetupTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupTurn")
            .field("draft_id", &self.draft_id)
            .field("message_id", &self.message_id)
            .field("sequence", &self.sequence)
            .field("reassigned", &self.reassigned)
            .finish_non_exhaustive()
    }
}

/// The draft's head candidate as an assignment carries it.
#[derive(Debug, Clone)]
pub struct SetupHead {
    /// The candidate a report's base names.
    pub id: Uuid7,
    /// Its revision, which a report's base names too.
    pub revision: u64,
    /// Its Definition, as JSON.
    pub definition: serde_json::Value,
}

impl TryFrom<Candidate> for SetupHead {
    type Error = crate::Error;

    fn try_from(candidate: Candidate) -> Result<Self> {
        Ok(Self {
            revision: candidate
                .revision
                .parse()
                .map_err(crate::error::raise::stored_number("revision"))?,
            definition: serde_json::to_value(&candidate.definition)
                .map_err(crate::error::raise::stored)?,
            id: candidate.id,
        })
    }
}
