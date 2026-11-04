//! Source identity is proved from native admissions, never model prose.

use afd_admission::Producer;
use afd_wire::task::TaskLimits;
use sqlx::PgConnection;

use super::super::{PreparedPlan, TaskLease, TaskRefusal, guard::LeaseRow, sql};
use crate::error::{Result, query};

#[derive(sqlx::FromRow)]
pub(in crate::task) struct Source {
    actor: String,
    producer: String,
    pub(in crate::task) request_json: String,
    pub(super) reply_provider: Option<String>,
    pub(super) reply_address: Option<String>,
    input_claim_lease_id: Option<String>,
    input_claim_fence: Option<i64>,
}

pub(super) async fn lock(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    current: &LeaseRow,
    plan: &PreparedPlan,
) -> Result<Source> {
    let source = load(connection, current, &plan.source_admission_id).await?;
    check_claim(&source, lease, current, &plan.source_admission_id)?;
    Ok(source)
}

pub(in crate::task) async fn load(
    connection: &mut PgConnection,
    current: &LeaseRow,
    admission: &afd_core::id::Uuid7,
) -> Result<Source> {
    let source: Source = sqlx::query_as(sql::LOCK_SOURCE)
        .bind(admission.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(connection)
        .await
        .map_err(query("lock durable task source"))?
        .ok_or(TaskRefusal::NotFound)?;
    Ok(source)
}

pub(in crate::task) fn check_claim(
    source: &Source,
    lease: TaskLease<'_>,
    current: &LeaseRow,
    admission: &afd_core::id::Uuid7,
) -> Result<()> {
    let owns_claim = source.input_claim_lease_id.as_deref() == Some(lease.lease_id.as_str())
        && source.input_claim_fence == Some(current.fence);
    if admission.as_str() != current.source_admission_id && !owns_claim {
        return Err(TaskRefusal::StaleState.into());
    }
    Ok(())
}

pub(in crate::task) fn check_human(source: &Source) -> Result<()> {
    // The Slack person-link adapter is added at the input boundary; no machine
    // actor is allowed to stand in for a person while that adapter is absent.
    if source.producer != Producer::Steer.as_str()
        || !afd_events::is_steer_actor(&source.actor)
        || source.actor == afd_events::ACTOR_MACHINE
    {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok(())
}

pub(super) fn check(source: &Source, current: &LeaseRow, plan: &PreparedPlan) -> Result<()> {
    check_human(source)?;
    if current.revision_id.as_deref() != Some(plan.accepted_revision_id.as_str()) {
        return Err(TaskRefusal::StaleState.into());
    }
    let message = afd_wire::event::message_of(&source.request_json);
    for item in &plan.items {
        let start = usize::try_from(item.source_span.start).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                column: "source_start",
                source,
            })
        })?;
        let end = usize::try_from(item.source_span.end).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                column: "source_end",
                source,
            })
        })?;
        if message.get(start..end).is_none() {
            return Err(TaskRefusal::ReplayConflict.into());
        }
        check_limits(item.limits)?;
    }
    Ok(())
}

fn check_limits(limits: TaskLimits) -> Result<()> {
    let ceiling = TaskLimits::default();
    if limits.model_nanos <= 0
        || limits.model_nanos > ceiling.model_nanos
        || limits.wall_seconds == 0
        || limits.wall_seconds > ceiling.wall_seconds
        || limits.running_seconds == 0
        || limits.running_seconds > ceiling.running_seconds
        || limits.leases == 0
        || limits.leases > ceiling.leases
        || limits.publications == 0
        || limits.publications > ceiling.publications
        || limits.decision_calls == 0
        || limits.decision_calls > ceiling.decision_calls
        || limits.fallback_reads == 0
        || limits.fallback_reads > ceiling.fallback_reads
    {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    Ok(())
}
