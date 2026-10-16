//! Immutable content and its reviewed authority advance under the fleet row lock.

use super::super::{
    ACTIVE, CONTEXT, Creations, INSTALLING, read,
    source::Captured,
    sql,
    stored::{Accepted, Stored},
};
use crate::{
    Result,
    error::raise::{query, stored},
    types::Review,
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::PgConnection;

const UPDATE: &str =
    "UPDATE core.fleets SET current_definition_revision_id=$2::uuid,setup_version=setup_version+1,
setup_context=$3::jsonb,setup_runtime_model_ref=$4,status=$5,config_json=$6::jsonb,
resource_version=resource_version+1,updated_at=$7 WHERE id=$1::uuid";
const REVIEW: &str = "UPDATE core.fleets SET setup_context=$2::jsonb WHERE id=$1::uuid";
const CLOSE: &str = "UPDATE core.authoring_drafts SET state=$2,closed_at=$3,expires_at=$3+$4,
resource_version=resource_version+1,updated_at=$3 WHERE id=$1::uuid";
const RETENTION: i64 = 30 * 24 * 60 * 60 * 1000;

impl Creations {
    pub(super) async fn write_improvement(
        &self,
        connection: &mut PgConnection,
        row: &Stored,
        captured: &Captured,
        checked: &Review,
        revision: &Uuid7,
        now: UnixMillis,
    ) -> Result<()> {
        let number = row
            .setup_version
            .checked_add(1)
            .ok_or_else(|| crate::error::raise::invalid("Setup revision is exhausted"))?;
        insert_revision(connection, row, captured, checked, revision, number, now).await?;
        let mut accepted = Accepted::from_review(captured, checked);
        let status = if matches!(row.status.as_str(), ACTIVE | INSTALLING) {
            INSTALLING
        } else {
            &row.status
        };
        let head = Head {
            revision,
            accepted: &accepted,
            status,
        };
        advance_fleet(connection, row, captured, checked, head, now).await?;
        self.replace_schedules(
            connection,
            &row.workspace_id,
            &row.id,
            number,
            &captured.candidate.definition,
            now,
        )
        .await?;
        let current = read::lock(connection, &row.workspace_id, &row.id).await?;
        accepted.review_digest = self.review(connection, &current).await?.review_digest;
        sqlx::query(REVIEW)
            .bind(row.id.as_str())
            .bind(serde_json::to_string(&accepted).map_err(stored)?)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        settle_draft(connection, row, captured, status, now).await
    }
}

/// The immutable revision row: the candidate's definition, digests and the
/// model the review settled on, numbered one past the fleet's setup version.
async fn insert_revision(
    connection: &mut PgConnection,
    row: &Stored,
    captured: &Captured,
    checked: &Review,
    revision: &Uuid7,
    number: i64,
    now: UnixMillis,
) -> Result<()> {
    sqlx::query(sql::REVISION)
        .bind(revision.as_str())
        .bind(row.id.as_str())
        .bind(number)
        .bind(serde_json::to_string(&captured.candidate.definition).map_err(stored)?)
        .bind(&captured.candidate.canonical_digest)
        .bind(&captured.candidate.policy_semantics)
        .bind(captured.library_revision.as_ref().map(Uuid7::as_str))
        .bind(now.as_millis())
        .bind(&checked.effective.runtime_model_ref)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

/// What the fleet row is moved onto: the new revision, the setup context it
/// was accepted under, and the status the improvement leaves it in.
struct Head<'a> {
    revision: &'a Uuid7,
    accepted: &'a Accepted,
    status: &'a str,
}

/// Points the fleet at the new revision before its schedules are replaced, so
/// the schedule rows and the review read back the setup they belong to.
async fn advance_fleet(
    connection: &mut PgConnection,
    row: &Stored,
    captured: &Captured,
    checked: &Review,
    head: Head<'_>,
    now: UnixMillis,
) -> Result<()> {
    sqlx::query(UPDATE)
        .bind(row.id.as_str())
        .bind(head.revision.as_str())
        .bind(serde_json::to_string(head.accepted).map_err(stored)?)
        .bind(&checked.effective.runtime_model_ref)
        .bind(head.status)
        .bind(serde_json::to_string(captured.candidate.definition.config()).map_err(stored)?)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

/// The private draft's last word: linked as `saving` while the fleet installs,
/// closed under the retention window otherwise.
async fn settle_draft(
    connection: &mut PgConnection,
    row: &Stored,
    captured: &Captured,
    status: &str,
    now: UnixMillis,
) -> Result<()> {
    let draft = &captured
        .draft
        .as_ref()
        .ok_or(crate::error::ErrorKind::Hidden)?
        .id;
    if status == INSTALLING {
        sqlx::query(sql::LINK)
            .bind(draft.as_str())
            .bind(row.id.as_str())
            .bind("saving")
            .bind(now.as_millis())
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
    } else {
        sqlx::query(CLOSE)
            .bind(draft.as_str())
            .bind("closed")
            .bind(now.as_millis())
            .bind(RETENTION)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}
