//! Notices contain bounded metadata; neither transcript nor provider output enters a bus.

use crate::{
    Result,
    draft::value,
    error::raise::query,
    types::{Failure, MessageState},
};
use afd_core::id::Uuid7;
use afd_wire::authoring::{FailureReason, Usage};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

const MESSAGES: &str = "\
SELECT json_build_object('sequence',sequence,'state',state,'candidate_id',candidate_id,
'usage',usage,'failure_reason',failure_reason)::text FROM core.authoring_messages
WHERE draft_id=$1::uuid ORDER BY sequence DESC LIMIT $2";
const LIMIT: i64 = 25;

/// One safe change hint. Its event name is carried by the stream framing.
#[derive(Debug, Serialize)]
pub struct Notice {
    /// Bounded semantic event name.
    #[serde(skip)]
    pub event: &'static str,
    /// Creator-private draft identity.
    pub draft_id: Uuid7,
    /// User request sequence, when this is a message hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    /// Original immutable candidate identity, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<Uuid7>,
    /// Draft-local candidate revision, when this is a head change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Current durable message state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<MessageState>,
    /// Known accounting or explicitly unknown provider figures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Daemon-derived cause and recovery action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
}

impl Notice {
    pub(super) fn candidate(draft: &Uuid7, candidate: &Uuid7, revision: i64) -> Self {
        Self {
            candidate_id: Some(candidate.clone()),
            revision: Some(revision.to_string()),
            ..Self::identity(draft, "candidate.changed")
        }
    }

    pub(super) fn closed(draft: &Uuid7) -> Self {
        Self::identity(draft, "draft.closed")
    }

    fn identity(draft: &Uuid7, event: &'static str) -> Self {
        Self {
            event,
            draft_id: draft.clone(),
            sequence: None,
            candidate_id: None,
            revision: None,
            state: None,
            usage: None,
            failure: None,
        }
    }
}

#[derive(Deserialize)]
struct Stored {
    sequence: i64,
    state: MessageState,
    candidate_id: Option<Uuid7>,
    usage: Option<Usage>,
    failure_reason: Option<FailureReason>,
}

pub(super) async fn messages(connection: &mut PgConnection, draft: &Uuid7) -> Result<Vec<Notice>> {
    let rows: Vec<String> = sqlx::query_scalar(MESSAGES)
        .bind(draft.as_str())
        .bind(LIMIT)
        .fetch_all(connection)
        .await
        .map_err(query(super::CONTEXT))?;
    rows.into_iter()
        .rev()
        .map(|raw| {
            let stored: Stored = value(&raw)?;
            let event = match stored.state {
                MessageState::Queued => "message.accepted",
                MessageState::Running => "message.running",
                MessageState::Completed => "message.completed",
                MessageState::Cancelled => "message.cancelled",
                MessageState::Failed => "message.failed",
                MessageState::TimedOut => "message.timed_out",
            };
            Ok(Notice {
                event,
                draft_id: draft.clone(),
                sequence: Some(stored.sequence.to_string()),
                candidate_id: stored.candidate_id,
                revision: None,
                state: Some(stored.state),
                usage: stored.usage,
                failure: stored.failure_reason.map(Into::into),
            })
        })
        .collect()
}
