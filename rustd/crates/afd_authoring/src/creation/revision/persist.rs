//! Immutable content and its reviewed authority advance under the fleet row lock.

use super::super::{
    ACTIVE, CONTEXT, Creations, INSTALLING, PRIVATE_RETENTION,
    reserve::{Head, insert_revision},
    source::Captured,
    sql,
    stored::{Accepted, Stored},
};
use crate::{
    Result,
    error::raise::query,
    types::{DraftState, Review},
};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::PgConnection;

impl Creations {
    pub(in crate::creation) async fn write_improvement(
        &self,
        connection: &mut PgConnection,
        row: &Stored,
        captured: &Captured,
        checked: &Review,
        revision: &Uuid7,
        now: Timestamp,
    ) -> Result<afd_events::activity::Announcement> {
        let number = row
            .head_revision
            .checked_add(1)
            .ok_or_else(|| crate::error::raise::invalid("Setup revision is exhausted"))?;
        let accepted = Accepted::from_review(captured, checked);
        let head = Head {
            origin: captured.origin,
            id: revision,
            number,
            accepted: &accepted,
            model: checked.effective.runtime_model_ref.as_deref(),
        };
        let announcement =
            insert_revision(connection, &row.id, &captured.candidate.definition, head).await?;
        let status = if matches!(row.status.as_str(), ACTIVE | INSTALLING) {
            INSTALLING
        } else {
            &row.status
        };
        sqlx::query(sql::ADVANCE)
            .bind(row.id.as_str())
            .bind(revision.as_str())
            .bind(status)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        self.replace_schedules(
            connection,
            &row.workspace_id,
            &row.id,
            number,
            &captured.candidate.definition,
            now,
        )
        .await?;
        super::activity::record(
            connection,
            row,
            checked.effective.runtime_model_ref.as_deref(),
            revision,
            number,
        )
        .await?;
        settle_draft(connection, row, captured, status).await?;
        Ok(announcement)
    }
}

/// The private draft's last word: linked as `saving` while the fleet installs,
/// closed under the retention window otherwise.
async fn settle_draft(
    connection: &mut PgConnection,
    row: &Stored,
    captured: &Captured,
    status: &str,
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
            .bind(DraftState::Saving.as_str())
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
    } else {
        sqlx::query(sql::CLOSE_DRAFT)
            .bind(draft.as_str())
            .bind(DraftState::Closed.as_str())
            .bind(PRIVATE_RETENTION)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}
