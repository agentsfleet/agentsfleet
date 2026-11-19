//! Prove the entire incorporated conversation prefix before publication or adoption.

use af_core::id::Uuid7;
use af_wire::lease::FoldRef;

use super::{TurnCheckpointRefusal, guard::TurnLeaseGuard, root, row::Plan};
use crate::{Result, error::query};

#[derive(sqlx::FromRow)]
struct ClaimRow {
    id: Uuid7,
    sequence: i64,
    input_claim_lease_id: Option<Uuid7>,
    input_claim_fence: Option<i64>,
    foldable: bool,
}

pub(super) async fn validate(
    owner: &mut TurnLeaseGuard<'_>,
    folds: &[FoldRef],
    writer: Option<(&Uuid7, i64)>,
) -> Result<Vec<Uuid7>> {
    let expected = root::fold_ids(folds)?;
    let Some(last) = folds.last() else {
        return Ok(expected);
    };
    let pattern = format!("{}%", afd_turns::view::ACTOR_PREFIX);
    let actual: Vec<ClaimRow> = sqlx::query_as(include_str!("sql/validate_folds.sql"))
        .bind(&owner.current.event_id)
        .bind(&owner.current.fleet_id)
        .bind(&owner.current.workspace_id)
        .bind(last.sequence)
        .bind(16_385_i64)
        .bind(afd_turns::Producer::Steer.as_str())
        .bind(afd_turns::view::ACTOR_MACHINE)
        .bind(pattern)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_all(&mut *owner.tx)
        .await
        .map_err(query("lock and validate exact checkpoint-covered inputs"))?;
    owner.fresh().await?;
    if actual.len() != folds.len()
        || actual.iter().zip(folds).any(|(row, saved)| {
            row.id.as_str() != saved.event_id.0
                || row.sequence != saved.sequence
                || !row.foldable
                || writer.is_some_and(|(lease, fence)| {
                    row.input_claim_lease_id.as_ref() != Some(lease)
                        || row.input_claim_fence != Some(fence)
                })
                || row
                    .input_claim_fence
                    .is_some_and(|fence| fence > owner.current.fence)
        })
    {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(expected)
}

pub(super) async fn adopt(owner: &mut TurnLeaseGuard<'_>, plan: &Plan) -> Result<Vec<FoldRef>> {
    let folds = plan.folds()?;
    let ids = validate(owner, &folds, None).await?;
    let changed: Vec<Uuid7> = sqlx::query_scalar(include_str!("sql/adopt_folds.sql"))
        .bind(&ids)
        .bind(&owner.current.id)
        .bind(owner.current.fence)
        .bind(&owner.current.fleet_id)
        .bind(&owner.current.workspace_id)
        .fetch_all(&mut *owner.tx)
        .await
        .map_err(query(
            "adopt only authenticated checkpoint input membership",
        ))?;
    if changed.len() != ids.len() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(folds)
}
