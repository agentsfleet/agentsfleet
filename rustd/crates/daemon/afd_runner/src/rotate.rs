//! Atomic runner credential rotation.

use af_core::id::Uuid7;
use afd_api_wire::admin::AdminState;
use jiff::Timestamp;

use crate::admin::state_wire;
use crate::error::{Result, query, rejected, runner_not_found, tenant_managed};
use crate::scope::{Owner, RunnerScope};
use crate::sql;
use crate::store::Runners;
use afd_auth::credential::CredentialKind;
use afd_auth::minted::Minted;

const CONTEXT_ROTATE: &str = "runner token rotation";
const DETAIL_REVOKED: &str = "revoked runners cannot rotate credentials";
const EVENT_TOKEN_ROTATED: &str = "runner_token_rotated";

impl Runners {}

impl RunnerScope<'_> {
    /// Replaces a runner's stored digest and returns the new credential once.
    ///
    /// The credential value exists only in [`Minted`]; the statement receives
    /// its digest. Rotation and the audit row are one statement, so neither can
    /// become visible without the other. `now` mints the audit row's identifier;
    /// the database stamps both rows.
    ///
    /// # Errors
    /// Refuses a missing or revoked runner and reports entropy or datastore
    /// failures without exposing either credential.
    pub async fn rotate_token(
        &self,
        runner: &Uuid7,
        actor_id: &str,
        now: Timestamp,
    ) -> Result<Minted> {
        let token = Minted::draw(CredentialKind::RunnerToken, self.runners.entropy())?;
        let event_id = self.runners.admin_event_id(now)?;
        let mut connection = self.runners.pool().acquire().await?;
        let changed: Option<bool> = sqlx::query_scalar(sql::runner::ROTATE_RUNNER_TOKEN)
            .bind(runner)
            .bind(token.digest().as_str())
            .bind(state_wire(AdminState::Revoked))
            .bind(event_id)
            .bind(EVENT_TOKEN_ROTATED)
            .bind(sql::meta::ACTOR_ID)
            .bind(actor_id)
            .bind(self.owner.tenant())
            .bind(self.owner.platform())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ROTATE))?;
        let Some(changed) = changed else {
            return Err(refused_rotation(self.owner, runner, &mut connection).await);
        };
        if !changed {
            return Err(rejected(DETAIL_REVOKED));
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let runner_id = runner.as_str();
        {
            let (event, message) = (
                &EVENT_TOKEN_ROTATED,
                format_args!("runner credential rotated"),
            );
            tracing::debug!(message, runner_id, event);
        };
        Ok(token)
    }
}

/// Why a rotation matched no row: no such runner in this scope, or a platform
/// caller addressing a tenant's runner. Read only on the refusal path.
async fn refused_rotation(
    owner: Owner<'_>,
    runner: &Uuid7,
    connection: &mut sqlx::PgConnection,
) -> crate::error::Error {
    if !owner.platform() {
        return runner_not_found();
    }
    let platform_owned: Result<Option<bool>> =
        sqlx::query_scalar(sql::runner::SELECT_RUNNER_PLATFORM_OWNED)
            .bind(runner)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_ROTATE));
    match platform_owned {
        Ok(Some(false)) => tenant_managed(),
        Ok(_) => runner_not_found(),
        Err(error) => error,
    }
}
