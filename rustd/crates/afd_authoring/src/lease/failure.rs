//! Retry actions follow daemon observations, never a runner's chosen label.

use afd_core::error_code;
use afd_wire::authoring::FailureReason;
use sqlx::PgConnection;

use super::{guard::Attempt, report::verdict};
use crate::{Error, Result, error::raise::query, funnel::Owed};

const CONTEXT: &str = "record authoring failure evidence";
const FAILED: &str = "failed";
const UNRESOLVED: &str = "\
SELECT EXISTS(SELECT 1 FROM fleet.authoring_dispatches
WHERE attempt_id=$1::uuid AND status IN ('pending','unknown'))";
const ATTEMPT: &str = "\
UPDATE fleet.authoring_attempts SET daemon_failure=COALESCE(daemon_failure,$2),updated_at=$3
WHERE id=$1::uuid";
const MESSAGE: &str = "\
UPDATE core.authoring_messages SET state=$3,failure_reason=$4,finished_at=$5,updated_at=$5
WHERE draft_id=$1::uuid AND sequence=$2 AND state=$6";

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
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(ATTEMPT)
        .bind(attempt.id.as_str())
        .bind(verdict::reason(reason))
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let unresolved: bool = sqlx::query_scalar(UNRESOLVED)
        .bind(attempt.id.as_str())
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let visible = if unresolved {
        FailureReason::ProviderUsageUnknown
    } else {
        attempt.daemon_failure.unwrap_or(reason)
    };
    let changed = sqlx::query(MESSAGE)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(FAILED)
        .bind(verdict::reason(visible))
        .bind(now)
        .bind(crate::types::RUNNING)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if changed.rows_affected() == 1 {
        owed.finished(connection, attempt.into(), FAILED).await?;
    }
    Ok(true)
}
