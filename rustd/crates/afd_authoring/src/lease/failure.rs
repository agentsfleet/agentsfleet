//! Retry actions follow daemon observations, never a runner's chosen label.

use afd_core::error_code;
use afd_wire::authoring::FailureReason;
use sqlx::PgConnection;

use super::{guard::Attempt, report::verdict, sql};
use crate::{
    Error, Result, error::raise::query, funnel::Owed, model::dispatch_status, types::MessageState,
};

const CONTEXT: &str = "record authoring failure evidence";
const FAILED: &str = MessageState::Failed.as_str();

/// Returns whether this refusal has an owner action backed by daemon evidence.
///
/// The message's finish is owed to `owed`, for the caller to report once its
/// transaction commits.
pub(crate) async fn record(
    connection: &mut PgConnection,
    attempt: &Attempt,
    failure: &Error,
    owed: &mut Owed,
) -> Result<bool> {
    let reason = match failure.code() {
        error_code::AUTHORING_MODEL => FailureReason::ModelUnavailable,
        error_code::AUTHORING_BALANCE => FailureReason::InsufficientBalance,
        error_code::AUTHORING_TOKEN => FailureReason::ContextTooLarge,
        _ => return Ok(false),
    };
    sqlx::query(sql::RECORD_FAILURE)
        .bind(attempt.id.as_str())
        .bind(verdict::reason(reason))
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let unresolved: bool = sqlx::query_scalar(sql::UNRESOLVED)
        .bind(attempt.id.as_str())
        .bind(dispatch_status::PENDING)
        .bind(dispatch_status::UNKNOWN)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let visible = if unresolved {
        FailureReason::ProviderUsageUnknown
    } else {
        attempt.failure_reason.unwrap_or(reason)
    };
    // Late refusals retain attempt evidence even when Stop already finished
    // the message. Only a real transition earns its terminal event.
    let changed = sqlx::query(crate::sql::message::FAIL)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(FAILED)
        .bind(verdict::reason(visible))
        .bind(crate::types::RUNNING)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if changed.rows_affected() == 1 {
        owed.finished(connection, attempt.into(), FAILED).await?;
    }
    Ok(true)
}
