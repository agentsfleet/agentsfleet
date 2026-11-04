//! Late accounting is authorized by retained original ownership, not a live task.

use afd_core::id::Uuid7;
use afd_wire::task::{
    DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult, DecisionRecordDisposition,
};
use sqlx::PgConnection;

use super::{
    account,
    journal::{Recording, Written},
    row::{self, Call, Lookup, Payload},
    semantic, sql,
};
use crate::{
    Result,
    error::{Error, ErrorKind, query, stale_fence},
    task::{TaskLease, TaskRefusal, TaskScope, Tasks, transaction},
};

pub(super) async fn record(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    id: &Uuid7,
    input: &garde::Valid<DecisionReceiptInput>,
) -> Result<DecisionReceiptResult> {
    if input.fencing_token != lease.fencing_token {
        return Err(stale_fence());
    }
    tokio::time::timeout(
        super::OPERATION_TIMEOUT,
        run(tasks, lease, id, &input.receipt),
    )
    .await
    .map_err(|source| Error::from(ErrorKind::DecisionDeadline { source }))?
}

async fn run(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    id: &Uuid7,
    receipt: &DecisionReceipt,
) -> Result<DecisionReceiptResult> {
    let digest = afd_core::etag::sha256_hex(
        &serde_json::to_vec(receipt).map_err(afd_core::json::Unreadable::from)?,
    );
    let (written, payload) = commit(tasks, lease, id, receipt, &digest).await?;
    after_commit(tasks, written, payload, receipt).await
}

async fn after_commit(
    tasks: &Tasks,
    written: Written,
    payload: Option<Payload>,
    receipt: &DecisionReceipt,
) -> Result<DecisionReceiptResult> {
    let disposition = disposition(tasks, &written.call, payload.as_ref(), receipt).await;
    changed(tasks, &written).await?;
    if let Some(settlement) = &written.settlement
        && let Err(fault) = tasks.dependencies.biller.settle(settlement).await
    {
        tracing::warn!(event = "lease_settle_deferred", work_id = written.call.original_lease_id.as_str(),
                error_code = %fault.code(), reason = "decision_settlement_pending");
    }
    let result = response(&written, disposition)?;
    tracing::info!(event = "dt_decision_recorded", decision_call_id = result.call_id.as_str(),
        assessment_id = written.call.assessment_id.as_str(), state = result.state.as_str(),
        call_age_ms = afd_core::clock::now().duration_since(written.call.created_at).as_millis(),
        purpose = written.call.purpose.as_str(), replayed = result.replayed,
        disposition = ?result.disposition, charged_nanos = result.charged_nanos.as_deref(),
        reserved_nanos = result.reserved_nanos.as_str());
    Ok(result)
}

async fn commit(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    id: &Uuid7,
    receipt: &DecisionReceipt,
    digest: &str,
) -> Result<(Written, Option<Payload>)> {
    let call = row::peek(tasks.leases.pool(), Lookup::Id(id.as_str()))
        .await?
        .ok_or(TaskRefusal::NotFound)?;
    if !call.owned_by(lease) {
        return Err(stale_fence());
    }
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let result = commit_on(&mut connection, tasks, lease, &call, receipt, digest).await;
    transaction::recycle(connection);
    result
}

async fn commit_on(
    connection: &mut PgConnection,
    tasks: &Tasks,
    lease: TaskLease<'_>,
    call: &Call,
    receipt: &DecisionReceipt,
    digest: &str,
) -> Result<(Written, Option<Payload>)> {
    let mut transaction = transaction::begin(connection).await?;
    // Match lifecycle lock order, including when either live parent is gone.
    lock_parents(&mut transaction, call).await?;
    let call = owned(&mut transaction, lease, &Call::id(&call.id, "id")?).await?;
    let plan = account::classify(&call, receipt, digest)?;
    let written = Recording {
        call: &call,
        receipt,
        digest,
    }
    .write(&mut transaction, tasks.leases.entropy(), plan)
    .await?;
    let payload = row::payload(&mut transaction, &call.id).await?;
    transaction
        .commit()
        .await
        .map_err(query("commit decision charge and original-owner receipt"))?;
    Ok((written, payload))
}

async fn owned(connection: &mut PgConnection, lease: TaskLease<'_>, id: &Uuid7) -> Result<Call> {
    let call = row::find(connection, Lookup::Id(id.as_str()))
        .await?
        .ok_or(TaskRefusal::NotFound)?;
    if !call.owned_by(lease) {
        return Err(stale_fence());
    }
    Ok(call)
}

async fn lock_parents(connection: &mut PgConnection, call: &Call) -> Result<()> {
    sqlx::query_scalar::<_, String>(sql::LOCK_PARENT)
        .bind(&call.fleet_id)
        .bind(&call.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("protect any remaining decision parent"))?;
    // Serialize actual charges with issuance's budget check, even for intent
    // calls without a task. Erased parents remain optional for late accounting.
    sqlx::query_scalar::<_, String>(crate::task::sql::LOCK_SCOPE_SLOT)
        .bind(&call.fleet_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(
            "serialize decision accounting with physical issuance",
        ))?;
    if let Some(task) = &call.task_id {
        sqlx::query_scalar::<_, String>(sql::LOCK_TASK)
            .bind(task)
            .bind(&call.fleet_id)
            .bind(&call.workspace_id)
            .fetch_optional(connection)
            .await
            .map_err(query("lock any remaining decision budget owner"))?;
    }
    Ok(())
}

async fn disposition(
    tasks: &Tasks,
    call: &Call,
    payload: Option<&Payload>,
    receipt: &DecisionReceipt,
) -> DecisionRecordDisposition {
    match (payload, receipt) {
        (Some(payload), DecisionReceipt::Provider { result }) => {
            semantic::disposition(tasks, call, &payload.request.0, result).await
        }
        _ => DecisionRecordDisposition::Unusable,
    }
}

async fn changed(tasks: &Tasks, written: &Written) -> Result<()> {
    if let Some((task, version)) = written.call.task_id.as_ref().zip(written.task_version) {
        let scope = TaskScope {
            fleet_id: Call::id(&written.call.fleet_id, "fleet_id")?,
            workspace_id: Call::id(&written.call.workspace_id, "workspace_id")?,
        };
        tasks
            .publish_changed(
                &scope,
                &Call::id(task, "task_id")?,
                crate::task::row::counter(version, "resource_version")?,
            )
            .await;
    }
    Ok(())
}

fn response(
    written: &Written,
    disposition: DecisionRecordDisposition,
) -> Result<DecisionReceiptResult> {
    let call = &written.call;
    Ok(DecisionReceiptResult {
        call_id: call.id.clone(),
        state: call.state()?,
        replayed: written.replayed,
        disposition,
        usage: call.known_usage.as_ref().map(|usage| usage.0),
        charged_nanos: call.charged_nanos.map(|charged| charged.to_string()),
        overage_nanos: call.charged_nanos.map(|charged| {
            charged
                .saturating_sub(call.reserved_nanos)
                .max(0)
                .to_string()
        }),
        reserved_nanos: if call.accounted {
            "0".to_owned()
        } else {
            call.reserved_nanos.to_string()
        },
    })
}
