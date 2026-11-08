//! The one admission statement, on whatever connection the caller holds.

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use sqlx::PgConnection;

use super::verdict::{self, Answer, Bound};
use crate::admit::{Ledger, ReplyBinds};
use crate::error::{Result, query};
use crate::{Admission, Admissions, sql};

const CONTEXT_ADMIT: &str = "admit one message";

/// What a new key is checked against beside the fleet's own row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Gates {
    /// The sampled deployment estimate, compared with the ceiling.
    pub(crate) estimate: u64,
    /// The status a new key needs; `None` admits a fleet in any status, as
    /// activation does while the fleet is still installing.
    pub(crate) runnable: Option<FleetStatus>,
}

impl Admissions {
    /// Runs `admit_one.sql` and names its answer.
    pub(crate) async fn insert_on(
        &self,
        connection: &mut PgConnection,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        gates: Gates,
    ) -> Result<Ledger> {
        let Gates { estimate, runnable } = gates;
        let limit = self.budgets.deployment_backlog;
        let reply = ReplyBinds::from(admission.reply);
        let answer = sqlx::query_as::<_, Answer>(sql::ADMIT_ONE)
            .bind(row_id.as_str())
            .bind(admission.fleet)
            .bind(admission.workspace)
            .bind(admission.producer.as_str())
            .bind(key)
            .bind(digest)
            .bind(admission.actor)
            .bind(admission.event_type.as_str())
            .bind(admission.request_json)
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .bind(i64::try_from(estimate).unwrap_or(i64::MAX))
            .bind(reply.connector)
            .bind(reply.address)
            .bind(reply.inherited.as_ref().map(Uuid7::as_str))
            .bind(af_fleet_runtime::definition::POLICY_SEMANTICS)
            .bind(runnable.map(FleetStatus::as_str))
            .bind(afd_state::sql::LEASE_STATUS_ACTIVE)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_ADMIT))?;
        verdict::decide(
            &answer,
            Bound {
                runnable,
                ceiling_open: estimate < limit,
                limit,
            },
        )
    }
}
