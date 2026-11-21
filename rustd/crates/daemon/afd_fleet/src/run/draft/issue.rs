//! Writing a draft's run under its assignment's fence.

use af_core::id::Uuid7;
use afd_billing::rates::Posture;
use afd_setup::setup_run::SetupTurn;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::assign::CONTEXT;
use crate::error::{Result, query};
use crate::run::{RunOwner, RunService, sql};

/// What [`RunService::issue_draft`] writes, by name.
pub(super) struct Issue<'a> {
    pub(super) runner: &'a Uuid7,
    pub(super) owner: &'a RunOwner,
    pub(super) turn: &'a SetupTurn,
    pub(super) fence: i64,
    pub(super) held_until: Timestamp,
    pub(super) run_version: i64,
    pub(super) reassignment: bool,
}

impl RunService {
    /// Writes the draft's run under its assignment's fence with
    /// [`sql::shared::INSERT_RUN`]: the run's id, or `None` when the host
    /// filled or the fence or the placement choice moved.
    pub(super) async fn issue_draft(
        &self,
        tx: &mut sqlx::PgConnection,
        issue: Issue<'_>,
        now: Timestamp,
    ) -> Result<Option<Uuid7>> {
        let run_id = self.runs.entropy().uuid7(now)?;
        let audit_id = self.runs.entropy().uuid7(now)?;
        let kind = if issue.reassignment {
            "reassigned"
        } else {
            "fresh"
        };
        let metadata = std::collections::BTreeMap::from([
            (afd_runner::sql::meta::RUN_ID, run_id.as_str()),
            (
                afd_runner::sql::meta::TURN_ID,
                issue.turn.message_id.as_str(),
            ),
            (afd_runner::sql::meta::KIND, kind),
        ]);
        let posture = if issue.turn.model.platform_managed {
            Posture::Platform
        } else {
            Posture::SelfManaged
        };
        let created: Option<jiff_sqlx::Timestamp> = sqlx::query_scalar(sql::shared::INSERT_RUN)
            .bind(&run_id)
            .bind(issue.runner)
            .bind(issue.owner.fleet_id())
            .bind(issue.owner.draft_id())
            .bind(&issue.turn.workspace_id)
            .bind(&issue.turn.tenant_id)
            .bind(&issue.turn.message_id)
            .bind(issue.turn.message_created_at.to_sqlx())
            .bind(posture.as_str())
            .bind(issue.turn.model.provider.as_ref())
            .bind(issue.turn.model.model.as_ref())
            .bind(issue.fence)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(&audit_id)
            .bind(afd_runner::sql::event_type::RUN_ACQUIRED)
            .bind(sqlx::types::Json(metadata))
            .bind(None::<sqlx::types::Json<serde_json::Value>>)
            .bind(issue.held_until.to_sqlx())
            .bind(issue.run_version)
            .bind(issue.owner.kind().as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        Ok(created.map(|_created| run_id))
    }
}
