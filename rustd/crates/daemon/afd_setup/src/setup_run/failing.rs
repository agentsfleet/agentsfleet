//! A setup turn an assignment fails rather than runs: why, the stored word for
//! it, and the one record an operator searches for.
//!
//! A fault that is the turn's own (its model, its payer's balance, a stored
//! row it reads) fails that turn and passes its draft, so the walk goes on to
//! the next draft and then to the runner's fleet work. A datastore or host
//! failure is not the turn's, and rolls the assignment back instead.

use af_core::{
    error_code::{self, ErrorCode},
    id::Uuid7,
};
use af_wire::setup::FailureReason;
use sqlx::PgConnection;

use super::{SetupAssignment, SetupOwed, sql};
use crate::{
    Result,
    error::raise::query,
    funnel::Message,
    types::{MessageState, QUEUED, RUNNING},
};

/// Logged at `warn` when an assignment fails a setup turn for a fault of its
/// own, with `error_code`, any `cause_error_code`, `draft_id` and `reason`.
pub const EVENT_TURN_FAILED: &str = "setup_turn_failed";

/// A message an assignment fails, from the status it holds.
pub(super) struct Failing<'a> {
    pub(super) message: Message<'a>,
    pub(super) sequence: i64,
    pub(super) running: bool,
}

impl Failing<'_> {
    /// Fails the message for `error` when it is the turn's own fault, logs
    /// it, and passes the draft.
    ///
    /// # Errors
    /// Hands back `error` itself when it is the datastore's or this host's,
    /// so the assignment rolls back; and a datastore that would not answer.
    pub(super) async fn refuse(
        &self,
        connection: &mut PgConnection,
        error: crate::Error,
        owed: &mut SetupOwed,
    ) -> Result<SetupAssignment> {
        let Some(reason) = error.turn_failure() else {
            return Err(error);
        };
        self.fail(connection, reason, owed).await?;
        log_turn_failed(self.message.draft, reason, error.code());
        Ok(SetupAssignment::Passed)
    }

    /// Fails the message with [`crate::sql::message::FAIL`] and owes its
    /// funnel record.
    pub(super) async fn fail(
        &self,
        connection: &mut PgConnection,
        reason: FailureReason,
        owed: &mut SetupOwed,
    ) -> Result<()> {
        let failed = MessageState::Failed.as_str();
        sqlx::query(crate::sql::message::FAIL)
            .bind(self.message.draft)
            .bind(self.sequence)
            .bind(failed)
            .bind(reason_word(reason))
            .bind(if self.running { RUNNING } else { QUEUED })
            .execute(&mut *connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        owed.0.finished(connection, self.message, failed).await
    }
}

/// Logs, at `warn`, a setup turn an assignment failed for `reason`.
///
/// `cause` is the failure's code. The record's own code is the reason's where
/// it has one, with the cause beside it, and the cause's otherwise.
pub fn log_turn_failed(draft: &Uuid7, reason: FailureReason, cause: ErrorCode) {
    let own = match reason {
        FailureReason::ModelUnavailable => error_code::SETUP_MODEL,
        FailureReason::InsufficientBalance => error_code::SETUP_BALANCE,
        FailureReason::ProviderFailed
        | FailureReason::ProviderUsageUnknown
        | FailureReason::FrameTooLarge => cause,
    };
    let error_code = own.as_str();
    let cause_error_code = (cause != own).then(|| cause.as_str());
    let (draft_id, reason) = (draft.as_str(), reason_word(reason));
    tracing::warn!(
        event = EVENT_TURN_FAILED,
        error_code,
        cause_error_code,
        draft_id,
        reason
    );
}

/// The word a turn's `failure_reason` stores for `value`, its wire spelling.
pub(crate) const fn reason_word(value: FailureReason) -> &'static str {
    match value {
        FailureReason::InsufficientBalance => "insufficient_balance",
        FailureReason::ModelUnavailable => "model_unavailable",
        FailureReason::ProviderFailed => "provider_failed",
        FailureReason::ProviderUsageUnknown => "provider_usage_unknown",
        FailureReason::FrameTooLarge => "frame_too_large",
    }
}
