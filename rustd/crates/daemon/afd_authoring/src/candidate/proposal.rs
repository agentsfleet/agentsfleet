//! The model edits the same native setup as the drawer, against an exact base.

use crate::{
    Drafts, Result, candidate, draft,
    error::{ErrorKind, raise::invalid},
    lease::{ActivityInput, Event, Proposal, guard::Guarded},
};
use af_fleet_runtime::{Definition, definition::EditBatch};
use af_wire::authoring::CandidateVersion;
use sqlx::PgConnection;

const OVERFLOW: &str = "The candidate revision cannot advance further";

pub(super) async fn apply(
    service: &Drafts,
    connection: &mut PgConnection,
    guarded: &Guarded,
    request: &garde::Valid<ActivityInput>,
) -> Result<Option<CandidateVersion>> {
    let Event::Proposal { proposal } = &request.event else {
        return Ok(None);
    };
    if matches!(proposal, Proposal::Edit { .. }) {
        let veto: bool = sqlx::query_scalar(include_str!("../model/edit_intent/sql/veto.sql"))
            .bind(guarded.attempt.id.as_str())
            .bind(guarded.draft.candidate_revision)
            .bind("no")
            .fetch_one(&mut *connection)
            .await
            .map_err(crate::error::raise::query("check setup edit veto"))?;
        if veto {
            return Err(invalid(
                "The newest message does not request a setup change",
            ));
        }
    }
    let definition = replacement(connection, guarded, proposal).await?;
    let revision = guarded
        .draft
        .candidate_revision
        .checked_add(1)
        .ok_or_else(|| invalid(OVERFLOW))?;
    let candidate = candidate::append(
        connection,
        candidate::Replacement {
            id: &service.mint(guarded.now)?,
            draft: &guarded.draft.id,
            revision,
            definition: &definition,
        },
    )
    .await?;
    Ok(Some(CandidateVersion {
        id: candidate.id.to_string(),
        revision: candidate.revision,
    }))
}

async fn replacement(
    connection: &mut PgConnection,
    guarded: &Guarded,
    proposal: &Proposal,
) -> Result<Definition> {
    let draft = &guarded.draft;
    match proposal {
        Proposal::Initialize { definition } if draft.head_candidate_id.is_none() => {
            definition.validate_initial_generation()?;
            Ok(definition.as_ref().clone())
        }
        Proposal::Edit {
            base_candidate_id,
            base_candidate_revision,
            config_ops,
            instruction_edits,
        } if draft
            .head_candidate_id
            .as_ref()
            .is_some_and(|id| id.as_str() == base_candidate_id)
            && draft.candidate_revision.to_string() == *base_candidate_revision =>
        {
            let id = draft.head_candidate_id.as_ref().ok_or(ErrorKind::Fence)?;
            let candidate = candidate::read(connection, &draft.id, id).await?;
            Ok(candidate.definition.edit(EditBatch {
                config_ops: config_ops.clone(),
                instruction_edits: instruction_edits.clone(),
            })?)
        }
        _ => Err(ErrorKind::Stale {
            etag: draft::etag(&draft.id, draft.resource_version),
        }
        .into()),
    }
}
