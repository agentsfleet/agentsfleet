//! The shared ledger statement runs on the caller's transaction without external I/O.

use crate::admit::{Ledger, ReplyBinds};
use crate::error::{Error, ErrorKind, Result, query};
use crate::{Admission, Admissions, BudgetScope, Repeated, logical_id, sql};
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::{PgConnection, Row as _};

const CONTEXT_ADMIT: &str = "record staged admission";
const NO_REPLAYS: i64 = 0;

impl Admissions {
    #[expect(
        clippy::too_many_arguments,
        reason = "the shared statement binds one immutable admission and its measured budget"
    )]
    pub(crate) async fn insert_on(
        &self,
        connection: &mut PgConnection,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        now: UnixMillis,
        estimate: u64,
    ) -> Result<Ledger> {
        super::policy::check(connection, admission, key).await?;
        let reply = ReplyBinds::from(admission.reply);
        let row = sqlx::query(sql::INSERT_ADMISSION)
            .bind(row_id.as_str())
            .bind(admission.fleet)
            .bind(admission.workspace)
            .bind(admission.producer.as_str())
            .bind(key)
            .bind(digest)
            .bind(admission.actor)
            .bind(admission.event_type.as_str())
            .bind(admission.request_json)
            .bind(now.as_millis())
            .bind(NO_REPLAYS)
            .bind(i64::try_from(self.budgets.replay_backlog).unwrap_or(i64::MAX))
            .bind(i64::try_from(estimate).unwrap_or(i64::MAX))
            .bind(reply.connector)
            .bind(reply.address)
            .bind(reply.inherit_created_at)
            .bind(reply.inherit_seq)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ADMIT))?
            .ok_or_else(|| {
                Error::from(ErrorKind::OverBudget {
                    scope: BudgetScope::Deployment,
                    limit: self.budgets.replay_backlog,
                })
            })?;
        let inserted: bool = row.try_get(0).map_err(query(CONTEXT_ADMIT))?;
        let created_at: i64 = row.try_get(1).map_err(query(CONTEXT_ADMIT))?;
        let seq: i64 = row.try_get(2).map_err(query(CONTEXT_ADMIT))?;
        let receipt: Option<String> = row.try_get(3).map_err(query(CONTEXT_ADMIT))?;
        let digest: String = row.try_get(4).map_err(query(CONTEXT_ADMIT))?;
        let fleet: String = row.try_get(5).map_err(query(CONTEXT_ADMIT))?;
        Ok(Ledger {
            stored: Repeated {
                id: logical_id(created_at, seq),
                digest,
                fleet,
                created_at,
            },
            inserted,
            receipt,
        })
    }
}
