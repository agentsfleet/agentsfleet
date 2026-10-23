//! The shared ledger statement runs on the caller's transaction without external I/O.

use crate::admit::{Ledger, ReplyBinds};
use crate::error::{Error, ErrorKind, Result, query};
use crate::{Admission, Admissions, BudgetScope, sql};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT_ADMIT: &str = "record staged admission";

impl Admissions {
    pub(crate) async fn insert_on(
        &self,
        connection: &mut PgConnection,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        estimate: u64,
    ) -> Result<Ledger> {
        super::policy::check(connection, admission, key).await?;
        self.check_fleet_budget(connection, admission, key).await?;
        let reply = ReplyBinds::from(admission.reply);
        sqlx::query_as::<_, Ledger>(sql::INSERT_ADMISSION)
            .bind(row_id.as_str())
            .bind(admission.fleet)
            .bind(admission.workspace)
            .bind(admission.producer.as_str())
            .bind(key)
            .bind(digest)
            .bind(admission.actor)
            .bind(admission.event_type.as_str())
            .bind(admission.request_json)
            .bind(i64::try_from(self.budgets.deployment_backlog).unwrap_or(i64::MAX))
            .bind(i64::try_from(estimate).unwrap_or(i64::MAX))
            .bind(reply.connector)
            .bind(reply.address)
            .bind(reply.inherited.as_ref().map(Uuid7::as_str))
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ADMIT))?
            .ok_or_else(|| {
                Error::from(ErrorKind::OverBudget {
                    scope: BudgetScope::Deployment,
                    limit: self.budgets.deployment_backlog,
                })
            })
    }
}
