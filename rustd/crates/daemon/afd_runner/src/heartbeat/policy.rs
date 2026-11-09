//! One native policy row also proves the opening credential and durable connection owner.

use af_core::id::Uuid7;
use serde_json::value::RawValue;
use sqlx::{PgConnection, types::Json};

use crate::{
    Result,
    capacity::authority::{Authority, Decision, Refusal},
    error::query,
    policy::{AssignmentColumns, StoredVerdict},
    sql,
};

pub(super) struct PolicyRow {
    pub(super) assignment: AssignmentColumns,
    pub(super) stored: StoredVerdict,
    pub(super) capability_report_json: Option<String>,
    pub(super) selftest_requested: bool,
}

#[derive(sqlx::FromRow)]
struct PolicyColumns {
    durable_tasks_enabled: bool,
    decision_model_id: Option<Uuid7>,
    sandbox_tier: String,
    network_policy: String,
    registry_allowlist: Json<Box<RawValue>>,
    worker_count: i32,
    degraded_reason: Option<String>,
    capability_report: Option<Json<Box<RawValue>>>,
    selftest_requested: bool,
    sandbox_config: Option<Json<Box<RawValue>>>,
    #[sqlx(flatten)]
    authority: ConnectionColumns,
}

#[derive(sqlx::FromRow)]
struct ConnectionColumns {
    credential_matches: bool,
    active: bool,
    owned: Option<bool>,
}

impl From<PolicyColumns> for PolicyRow {
    fn from(row: PolicyColumns) -> Self {
        Self {
            assignment: AssignmentColumns {
                durable_tasks_enabled: row.durable_tasks_enabled,
                decision_model_id: row.decision_model_id.map(String::from),
                sandbox_tier: row.sandbox_tier,
                network_policy: row.network_policy,
                registry_allowlist_json: row.registry_allowlist.0.get().to_owned(),
                worker_count: row.worker_count,
                sandbox_config_json: row.sandbox_config.map(|value| value.0.get().to_owned()),
            },
            stored: StoredVerdict {
                reason: row.degraded_reason,
            },
            capability_report_json: row.capability_report.map(|value| value.0.get().to_owned()),
            selftest_requested: row.selftest_requested,
        }
    }
}

pub(super) async fn read(
    connection: &mut PgConnection,
    runner: &Uuid7,
    authority: Option<&Authority>,
) -> Result<Decision<PolicyRow>> {
    let row = sqlx::query_as::<_, PolicyColumns>(sql::runner::SELECT_RUNNER_ASSIGNED_POLICY)
        .bind(runner)
        .bind(authority.map(|owner| owner.digest().as_str()))
        .bind(authority.map(Authority::connection))
        .bind(sql::RUNNER_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("runner policy and connection authority read"))?;
    Ok(match row {
        None
        | Some(PolicyColumns {
            authority:
                ConnectionColumns {
                    credential_matches: false,
                    ..
                },
            ..
        }) => Decision::Refused(Refusal::Credential),
        Some(PolicyColumns {
            authority: ConnectionColumns { active: false, .. },
            ..
        }) => Decision::Refused(Refusal::Revoked),
        Some(PolicyColumns {
            authority:
                ConnectionColumns {
                    owned: None | Some(false),
                    ..
                },
            ..
        }) => Decision::Refused(Refusal::Superseded),
        Some(row) => Decision::Allowed(row.into()),
    })
}
