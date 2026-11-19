//! The run a report is about, read for a report whose stream session kept
//! no facts from the offer. The report itself commits in one statement
//! ([`super::commit`]).

use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::error::{Result, query};
use crate::run::affinity::Fence;
use crate::run::sql;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_LOAD: &str = "report run load";

/// The run a report is about, as the row holds it.
///
/// Every identifier is a [`Uuid7`] rather than a string: the columns are
/// `uuid` and the finalize path passes them to four
/// more statements, so parsing once here is what stops a malformed one reaching
/// a `::uuid` cast four frames down and failing as a query error instead of as
/// the data fault it is.
#[derive(Debug, Clone)]
pub struct Reported {
    /// The fleet whose slot this run holds.
    pub fleet_id: Uuid7,
    /// The workspace the work belongs to.
    pub workspace_id: Uuid7,
    /// The tenant whose wallet the settle draws on.
    pub tenant_id: Uuid7,
    /// The event that was executed — the ledger's logical id.
    pub turn_id: Uuid7,
    /// Who or what raised the event.
    pub actor: String,
    /// The billing posture resolved at issue.
    pub posture: String,
    /// The provider resolved at issue.
    pub provider: String,
    /// The model resolved at issue.
    pub model: String,
    /// The token this run was issued under — what the settle is fenced on.
    pub fence: Fence,
    /// When the run was issued, on this daemon's clock.
    pub run_created_at: Timestamp,
    /// When the event it runs was created, as the run row copied it.
    pub turn_created_at: Timestamp,
    /// The catalogue version the report read beside the run; `None` prices
    /// from an uncached read.
    pub catalogue_version: Option<i64>,
}

impl Runs {
    /// The run `run_id` names, if it belongs to `runner_id`.
    ///
    /// `Ok(None)` for both "no such run" and "somebody else's run" — the
    /// statement's `runner_id` predicate cannot tell them apart, which is
    /// deliberate. See [`sql::report::SELECT_RUN_FOR_REPORT`].
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn load_for_report(
        &self,
        run_id: &Uuid7,
        runner_id: &Uuid7,
    ) -> Result<Option<Reported>> {
        let mut connection = self.pool().acquire().await?;
        let found: Option<RunForReport> = sqlx::query_as(sql::report::SELECT_RUN_FOR_REPORT)
            .bind(run_id)
            .bind(runner_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_LOAD))?;
        Ok(found.map(RunForReport::into_reported))
    }
}

/// [`sql::report::SELECT_RUN_FOR_REPORT`]'s row.
#[derive(sqlx::FromRow)]
struct RunForReport {
    fleet_id: Uuid7,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    turn_id: Uuid7,
    actor: String,
    posture: String,
    provider: String,
    model: String,
    fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    turn_created_at: Timestamp,
    catalogue_version: Option<i64>,
}

impl RunForReport {
    fn into_reported(self) -> Reported {
        Reported {
            fleet_id: self.fleet_id,
            workspace_id: self.workspace_id,
            tenant_id: self.tenant_id,
            turn_id: self.turn_id,
            actor: self.actor,
            posture: self.posture,
            provider: self.provider,
            model: self.model,
            fence: Fence::from_i64(self.fencing_token),
            run_created_at: self.created_at,
            turn_created_at: self.turn_created_at,
            catalogue_version: self.catalogue_version,
        }
    }
}
