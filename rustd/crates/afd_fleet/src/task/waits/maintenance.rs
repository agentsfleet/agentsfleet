//! Four native readers share one bounded due pass; no agent, restore or model call.

use super::{
    WaitMaintenance,
    claim::{self, Claimed},
    finish::{self, Probe},
    row::Candidate,
};
use crate::{
    Result,
    error::{ErrorKind, query},
    task::Tasks,
};
use afd_wire::task::TaskState;
use futures_util::{StreamExt as _, stream};
use tracing::Instrument as _;

pub(in crate::task) async fn maintain(tasks: &Tasks) -> Result<WaitMaintenance> {
    super::cleanup::maintain(tasks).await?;
    super::fanout::maintain(tasks).await?;
    let mut connection = tasks.leases.pool().acquire().await?;
    let candidates: Vec<Candidate> = sqlx::query_as(include_str!("sql/due.sql"))
        .bind(TaskState::Waiting.as_str())
        .bind(CANDIDATES)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("select bounded exact-head wait candidates"))?;
    drop(connection);
    let mut ready = stream::iter(candidates.into_iter().map(|candidate| async move {
        let Some(claimed) = claim::claim(tasks, &candidate).await? else {
            return Ok(None);
        };
        let Claimed {
            owner,
            claim,
            permit,
        } = claimed;
        let work = async {
            let github = tasks
                .dependencies
                .github
                .as_ref()
                .ok_or(crate::task::TaskRefusal::InvalidTarget)?;
            github
                .dependency_session(permit)
                .await?
                .status(&owner.target)
                .await
        };
        let timed = async {
            match tokio::time::timeout(claim::CLAIM_TTL, work).await {
                Ok(result) => result,
                Err(source) => Err(ErrorKind::TaskRemoteDeadline { source }.into()),
            }
        };
        let result = super::super::observe::measure(
            afd_observability::metrics::label::task::TaskOperation::WaitRead,
            timed,
            |_| afd_observability::metrics::label::task::TaskOutcome::Accepted,
        )
        .instrument(tracing::info_span!(
            "task_dependency_read",
            task_id = owner.task_id.as_str(),
            wait_id = owner.id.as_str()
        ))
        .await;
        Ok::<_, crate::Error>(Some(Probe {
            owner,
            claim,
            result,
        }))
    }))
    .buffer_unordered(READERS);
    let mut result = WaitMaintenance::default();
    while let Some(next) = ready.next().await {
        match next {
            Ok(Some(probe)) => {
                result.checked += 1;
                match finish::finish(tasks, probe).await {
                    Ok(true) => result.woken += 1,
                    Ok(false) => result.deferred += 1,
                    Err(fault) => {
                        result.deferred += 1;
                        tracing::warn!(event = "dt_wait_commit_deferred", error_code = %fault.code());
                    }
                }
            }
            Ok(None) => (),
            Err(fault) => {
                result.deferred += 1;
                tracing::warn!(event = "dt_wait_claim_deferred", error_code = %fault.code());
            }
        }
    }
    Ok(result)
}

const CANDIDATES: i64 = 64;
const READERS: usize = 4;
