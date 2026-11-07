//! Fair bounded recovery retains progress across contention and cancellation.

use std::time::Duration;

use af_observability::{metrics::label::task::DecisionRecoveryOutcome as Outcome, producers};
use af_wire::task::{DecisionNotSentCause, DecisionReceipt};
use sqlx::PgConnection;

use super::{
    account::Plan,
    journal::{Recording, Written},
    locks::Mode,
    record,
    row::{self, Call, Lookup},
    sql,
};
use crate::{
    Result,
    error::{Error, ErrorKind, query},
    task::{DecisionMaintenance, DecisionMaintenanceCursor, Tasks, fetch, transaction},
};

const BATCH_LIMIT: u16 = 100;
const PASS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(sqlx::FromRow)]
struct Candidate {
    id: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    cycle_cutoff: jiff::Timestamp,
    #[sqlx(try_from = "i64")]
    overdue_ms: u64,
}

pub(super) async fn maintain(
    tasks: &Tasks,
    cursor: &mut DecisionMaintenanceCursor,
) -> Result<DecisionMaintenance> {
    tokio::time::timeout(PASS_TIMEOUT, run(tasks, cursor))
        .await
        .map_err(|source| Error::from(ErrorKind::DecisionDeadline { source }))?
}

async fn run(tasks: &Tasks, cursor: &mut DecisionMaintenanceCursor) -> Result<DecisionMaintenance> {
    let candidates = candidates(tasks, cursor).await?;
    let exhausted = candidates.len() < usize::from(BATCH_LIMIT);
    let receipt = DecisionReceipt::NotSent {
        cause: DecisionNotSentCause::Preflight,
    };
    let digest = record::receipt_digest(&receipt)?;
    let mut swept = DecisionMaintenance::default();
    let mut first_failure = None;
    for candidate in candidates {
        swept.scanned += 1;
        swept.oldest_overdue_ms = swept.oldest_overdue_ms.max(candidate.overdue_ms);
        // Progress belongs to the caller even if this future is cancelled.
        cursor.cutoff = Some(candidate.cycle_cutoff);
        let (_, id) = cursor.after.insert((candidate.expires_at, candidate.id));
        let outcome = match process(tasks, id, &receipt, &digest, &mut swept).await {
            Ok(outcome) => outcome,
            Err(fault) => failure(fault, id, &mut swept, &mut first_failure)?,
        };
        producers::task::recovery_candidate_completed(
            outcome,
            Duration::from_millis(candidate.overdue_ms),
        );
    }
    if exhausted {
        *cursor = DecisionMaintenanceCursor::default();
    }
    completed(&swept, first_failure);
    Ok(swept)
}

async fn candidates(
    tasks: &Tasks,
    cursor: &mut DecisionMaintenanceCursor,
) -> Result<Vec<Candidate>> {
    let batch = scan(tasks, cursor).await?;
    if batch.is_empty() && cursor.after.is_some() {
        *cursor = DecisionMaintenanceCursor::default();
        return scan(tasks, cursor).await;
    }
    Ok(batch)
}

async fn scan(tasks: &Tasks, cursor: &DecisionMaintenanceCursor) -> Result<Vec<Candidate>> {
    fetch::rows(
        tasks.leases.pool(),
        sqlx::query_as(sql::EXPIRED_UNISSUED)
            .bind(i64::from(BATCH_LIMIT))
            .bind(
                cursor
                    .after
                    .as_ref()
                    .map(|(at, _)| jiff_sqlx::Timestamp::from(*at)),
            )
            .bind(cursor.after.as_ref().map(|(_, id)| id.as_str()))
            .bind(cursor.cutoff.map(jiff_sqlx::Timestamp::from)),
        None,
        "read bounded expired decision reservations",
    )
    .await
}

async fn process(
    tasks: &Tasks,
    id: &str,
    receipt: &DecisionReceipt,
    digest: &str,
    swept: &mut DecisionMaintenance,
) -> Result<Outcome> {
    let Some(written) = expire(tasks, id, receipt, digest).await? else {
        return Ok(Outcome::Obsolete);
    };
    swept.expired += 1;
    let decision_call_id = written.call.id.as_str();
    let original_lease_id = written.call.original_lease_id.as_str();
    tracing::debug!(
        event = "dt_decision_expired_unissued",
        decision_call_id,
        original_lease_id,
        reason = "not_issued_before_deadline",
        released_reservation_nanos = written.call.reserved_nanos
    );
    record::changed(tasks, &written).await?;
    Ok(Outcome::Expired)
}

fn failure(
    fault: Error,
    id: &str,
    swept: &mut DecisionMaintenance,
    first_failure: &mut Option<af_core::error_code::ErrorCode>,
) -> Result<Outcome> {
    // A busy lock answers as unavailable too (55P03), so it is read first: it
    // defers one candidate and never stops the sweep.
    let (outcome, reason) = if fault.is_decision_lock_busy() {
        swept.deferred += 1;
        (Outcome::Contended, "financial_lock_busy")
    } else if fault.is_decision_recovery_outage() {
        return Err(fault);
    } else {
        swept.failed += 1;
        let _first_error = first_failure.get_or_insert(fault.code());
        (Outcome::Failed, "candidate_failed")
    };
    let outcome_label = outcome.as_str();
    let error_code = fault.code();
    tracing::debug!(event = "dt_decision_recovery_deferred", decision_call_id = id,
        outcome = outcome_label, %error_code, reason);
    Ok(outcome)
}

fn completed(swept: &DecisionMaintenance, first_failure: Option<af_core::error_code::ErrorCode>) {
    if let Some(error_code) = first_failure {
        let error_code = error_code.as_str();
        tracing::warn!(
            event = "dt_decision_maintenance_completed",
            error_code,
            scanned = swept.scanned,
            expired = swept.expired,
            deferred = swept.deferred,
            failed = swept.failed,
            oldest_overdue_ms = swept.oldest_overdue_ms,
            reason = "candidate_failures"
        );
    } else if swept.scanned > 0 {
        tracing::info!(
            event = "dt_decision_maintenance_completed",
            scanned = swept.scanned,
            expired = swept.expired,
            deferred = swept.deferred,
            failed = swept.failed,
            oldest_overdue_ms = swept.oldest_overdue_ms,
            reason = "expired_unissued_reservations"
        );
    }
}

async fn expire(
    tasks: &Tasks,
    id: &str,
    receipt: &DecisionReceipt,
    digest: &str,
) -> Result<Option<Written>> {
    let Some(call) = row::peek(tasks.leases.pool(), Lookup::Id(id)).await? else {
        return Ok(None);
    };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let result = expire_on(&mut connection, tasks, &call, receipt, digest).await;
    transaction::recycle(connection);
    result
}

async fn expire_on(
    connection: &mut PgConnection,
    tasks: &Tasks,
    call: &Call,
    receipt: &DecisionReceipt,
    digest: &str,
) -> Result<Option<Written>> {
    let mut tx = transaction::begin(connection).await?;
    Mode::Nowait.parents(&mut tx, call).await?;
    let Some(locked) = row::find_with(&mut tx, Lookup::Id(&call.id), Mode::Nowait).await? else {
        return Ok(None);
    };
    // An obsolete discovery cannot refund an issued or already-accounted call.
    if locked.issued || locked.accounted || locked.remaining_ms != 0 {
        return Ok(None);
    }
    let written = Recording {
        call: &locked,
        receipt,
        digest,
    }
    .write(&mut tx, tasks.leases.entropy(), Plan::NotSent)
    .await?;
    tx.commit()
        .await
        .map_err(query("commit proved-unissued decision expiry"))?;
    Ok(Some(written))
}
