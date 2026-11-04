//! Retained financial identity is independent of cascading private payloads.

use afd_core::id::Uuid7;
use afd_wire::{
    decision::{DecisionAssignment, DecisionRequest, DecisionUsage},
    task::{DecisionCallState, DecisionReceipt, DecisionUnavailableCause},
};
use serde::Deserialize;
use sqlx::{PgConnection, Postgres, postgres::PgArguments, query::QueryAs, types::Json};

use super::{context::AssessedContext, quote::FrozenQuote, sql};
use crate::{
    Result,
    error::{Error, ErrorKind, query, row_malformed},
};

#[derive(Debug, sqlx::FromRow)]
pub(super) struct Call {
    pub id: String,
    pub original_runner_id: String,
    pub original_lease_id: String,
    pub source_admission_id: String,
    pub fleet_id: String,
    pub workspace_id: String,
    pub tenant_id: String,
    pub task_id: Option<String>,
    pub fencing_token: i64,
    pub request_digest: String,
    pub assessment_id: String,
    pub assessment_key: String,
    #[sqlx(try_from = "i16")]
    pub attempt_number: u8,
    pub retry_of_decision_call_id: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub logical_expires_at: jiff::Timestamp,
    pub purpose: String,
    pub status: String,
    pub provider: String,
    pub fleet_name: String,
    pub profile_snapshot: Json<DecisionAssignment>,
    pub price_snapshot: Json<FrozenQuote>,
    pub context_snapshot: Json<AssessedContext>,
    pub usage_sequence: i64,
    pub reserved_nanos: i64,
    pub issued: bool,
    pub accounted: bool,
    pub known_usage: Option<Json<DecisionUsage>>,
    pub charged_nanos: Option<i64>,
    pub canonical_receipt_digest: Option<String>,
    pub failure_receipt_digest: Option<String>,
    pub failure_reason: Option<Json<DecisionUnavailableCause>>,
    pub failure_age_ms: Option<i64>,
    pub remaining_ms: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: jiff::Timestamp,
}

#[derive(sqlx::FromRow)]
pub(super) struct Payload {
    pub request: Json<DecisionRequest>,
    pub result: Option<Json<DecisionReceipt>>,
}

#[derive(Clone, Copy)]
pub(super) enum Lookup<'a> {
    Id(&'a str),
    Operation { lease: &'a str, operation: &'a str },
    Assessment { key: &'a str, attempt: i16 },
}

pub(super) async fn find(
    connection: &mut PgConnection,
    lookup: Lookup<'_>,
) -> Result<Option<Call>> {
    find_with(connection, lookup, super::locks::Mode::Wait).await
}

pub(super) async fn find_with(
    connection: &mut PgConnection,
    lookup: Lookup<'_>,
    mode: super::locks::Mode,
) -> Result<Option<Call>> {
    statement(mode.statement(sql::FIND_CALL, sql::TRY_CALL), lookup)
        .fetch_optional(connection)
        .await
        .map_err(query("lock retained physical decision"))
}

pub(super) async fn peek(database: &afd_db::Db, lookup: Lookup<'_>) -> Result<Option<Call>> {
    Ok(crate::task::fetch::rows(
        database,
        statement(sql::PEEK_CALL, lookup),
        None,
        "read retained physical decision",
    )
    .await?
    .into_iter()
    .next())
}

fn statement<'a>(
    sql: &'static str,
    lookup: Lookup<'a>,
) -> QueryAs<'a, Postgres, Call, PgArguments> {
    let (id, lease, operation, key, attempt) = match lookup {
        Lookup::Id(id) => (Some(id), None, None, None, None),
        Lookup::Operation { lease, operation } => (None, Some(lease), Some(operation), None, None),
        Lookup::Assessment { key, attempt } => (None, None, None, Some(key), Some(attempt)),
    };
    sqlx::query_as(sql)
        .bind(id)
        .bind(lease)
        .bind(operation)
        .bind(key)
        .bind(attempt)
}

pub(super) async fn payload(connection: &mut PgConnection, call: &str) -> Result<Option<Payload>> {
    sqlx::query_as(sql::PAYLOAD)
        .bind(call)
        .fetch_optional(connection)
        .await
        .map_err(query("read private saved decision payload"))
}

pub(super) async fn saved_payload(database: &afd_db::Db, call: &str) -> Result<Option<Payload>> {
    Ok(crate::task::fetch::rows(
        database,
        sqlx::query_as(sql::PAYLOAD).bind(call),
        None,
        "read saved private decision data",
    )
    .await?
    .into_iter()
    .next())
}

impl Call {
    pub(super) fn state(&self) -> Result<DecisionCallState> {
        DecisionCallState::deserialize(serde::de::value::StrDeserializer::new(self.status.as_str()))
            .map_err(|source| Error::from(ErrorKind::TaskState { source }))
    }

    pub(super) fn id(text: &str, column: &'static str) -> Result<Uuid7> {
        Uuid7::parse(text).map_err(row_malformed("runner.decision_calls", column))
    }

    pub(super) fn owned_by(&self, lease: crate::task::TaskLease<'_>) -> bool {
        self.original_runner_id == lease.runner_id.as_str()
            && self.original_lease_id == lease.lease_id.as_str()
            && u64::try_from(self.fencing_token).ok() == Some(lease.fencing_token)
    }
}
