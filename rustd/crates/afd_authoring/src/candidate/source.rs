//! Source reads pin immutable native content and never infer a mutable head.

use crate::{
    Access, Result,
    access::Person,
    error::{
        ErrorKind,
        raise::{invalid, query, stored},
    },
    types::{CreateDraft, Source},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{ConfigOp, EditBatch};
use sqlx::PgConnection;

const CONTEXT: &str = "capture native setup source";
const PLATFORM: &str = "platform";
const REVISION_INVALID: &str = "setup_revision must be a positive decimal integer";

pub(crate) struct Captured {
    pub(crate) definition: Option<Definition>,
    pub(crate) fleet: Option<Uuid7>,
    /// The fleet revision a fleet source pinned, stored as the draft's source.
    pub(crate) fleet_revision: Option<Uuid7>,
    pub(crate) instance_name: Option<String>,
}

pub(crate) async fn resolve(
    connection: &mut PgConnection,
    access: Access<'_>,
    person: &Person,
    request: &garde::Valid<CreateDraft>,
) -> Result<Captured> {
    let value: Option<String> = match &request.source {
        Source::Blank => {
            return Ok(Captured {
                definition: None,
                fleet: None,
                fleet_revision: None,
                instance_name: None,
            });
        }
        Source::Definition { definition } => {
            return Ok(Captured {
                definition: Some(definition.as_ref().clone()),
                fleet: None,
                fleet_revision: None,
                instance_name: None,
            });
        }
        Source::Library {
            library_id,
            revision_id,
        } => sqlx::query_scalar(crate::sql::candidate::LIBRARY_SOURCE)
            .bind(library_id.as_str())
            .bind(revision_id.as_str())
            .bind(person.owner.as_str())
            .bind(PLATFORM)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?,
        Source::Fleet { fleet_id, .. } => {
            return fleet_source(
                connection,
                access.workspace,
                fleet_id,
                positive_revision(request)?,
            )
            .await;
        }
    };
    let definition = serde_json::from_str(&value.ok_or(ErrorKind::Hidden)?).map_err(stored)?;
    Ok(Captured {
        definition: Some(definition),
        fleet: None,
        fleet_revision: None,
        instance_name: None,
    })
}

fn positive_revision(request: &garde::Valid<CreateDraft>) -> Result<i64> {
    let Source::Fleet { setup_revision, .. } = &request.source else {
        return Err(invalid(REVISION_INVALID));
    };
    let revision = setup_revision
        .parse::<i64>()
        .map_err(|_overflow| invalid(REVISION_INVALID))?;
    if revision <= 0 || revision.to_string() != *setup_revision {
        return Err(invalid(REVISION_INVALID));
    }
    Ok(revision)
}

/// [`crate::sql::candidate::FLEET_SOURCE`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct FleetSource {
    id: String,
    definition: String,
    name: String,
    runtime_model_ref: Option<String>,
}

async fn fleet_source(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    revision: i64,
) -> Result<Captured> {
    let row: Option<FleetSource> = sqlx::query_as(crate::sql::candidate::FLEET_SOURCE)
        .bind(fleet.as_str())
        .bind(workspace.as_str())
        .bind(revision)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let row = row.ok_or(ErrorKind::Hidden)?;
    let stored_definition: Definition = serde_json::from_str(&row.definition).map_err(stored)?;
    let definition = match row.runtime_model_ref {
        Some(reference) if stored_definition.runtime().model().is_none() => stored_definition
            .edit(EditBatch {
                config_ops: vec![ConfigOp::Add {
                    path: "/config/x-agentsfleet/model".into(),
                    value: reference.into(),
                }],
                instruction_edits: Vec::new(),
            })?,
        _ => stored_definition,
    };
    Ok(Captured {
        definition: Some(definition),
        fleet: Some(fleet.clone()),
        fleet_revision: Some(Uuid7::parse(&row.id)?),
        instance_name: Some(row.name),
    })
}
