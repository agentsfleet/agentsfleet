//! The one report statement (M245 §6): its binder, in `$n` order beside the
//! text it orders, and the row it answers.

use af_core::id::Uuid7;
use afd_billing::price::RatedLine;
use afd_events::Closed;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::{FromRow as _, postgres::PgRow};

use crate::error::{Result, query};
use crate::lease::report::RunTokens;

/// Every write a terminal report makes, in one statement.
pub const COMMIT_REPORT: &str = include_str!("commit_report.sql");

/// The notice a durable task's settlement puts in the run's answer's place.
pub const APPLY_REPORT_NOTICE: &str = include_str!("apply_report_notice.sql");

/// What a query failure names the statement as.
const CONTEXT: &str = "one-statement report";

/// Logged when a settled run's token sums do not fit, and its metrics record
/// none.
const EVENT_RUN_TOKENS_UNREAD: &str = "report_run_tokens_unread";

/// The writes one report commits, bound in [`COMMIT_REPORT`]'s `$n` order.
#[derive(Debug)]
pub(crate) struct ReportStatement<'a> {
    pub(crate) lease_id: &'a Uuid7,
    pub(crate) runner_id: &'a Uuid7,
    pub(crate) fleet_id: &'a Uuid7,
    pub(crate) workspace_id: &'a Uuid7,
    pub(crate) succeeded: bool,
    /// The pricing facts the lines were priced under, which the statement
    /// guards on.
    pub(crate) posture: &'a str,
    pub(crate) provider: &'a str,
    pub(crate) model: &'a str,
    pub(crate) catalogue_version: Option<i64>,
    /// The runner's request lines, priced, and the id each is written under.
    pub(crate) lines: &'a [RatedLine],
    pub(crate) line_ids: &'a [Uuid7],
    /// The runtime line's tier and id; the statement measures and prices it.
    pub(crate) runtime_price: Option<Uuid7>,
    pub(crate) runtime_line_id: &'a Uuid7,
    pub(crate) status: &'a str,
    pub(crate) response_text: &'a str,
    pub(crate) tokens: i64,
    pub(crate) wall_ms: i64,
    pub(crate) failure_reason: Option<String>,
    pub(crate) failure_detail: Option<&'a str>,
    pub(crate) tool_calls: Option<&'a str>,
    pub(crate) checkpoint: &'a str,
    pub(crate) held_until: Option<Timestamp>,
    pub(crate) obligation_id: &'a Uuid7,
    pub(crate) obligation_receipt: &'a Uuid7,
    pub(crate) answer: &'a str,
    pub(crate) audit_id: &'a Uuid7,
    pub(crate) audit_metadata: serde_json::Value,
}

impl ReportStatement<'_> {
    /// Runs the statement and reads its one row.
    pub(crate) async fn run(&self, connection: &mut sqlx::PgConnection) -> Result<ReportAnswer> {
        let lines = self.lines;
        let column = |field: fn(&RatedLine) -> i64| lines.iter().map(field).collect::<Vec<i64>>();
        let row = sqlx::query(COMMIT_REPORT)
            .bind(self.lease_id)
            .bind(self.runner_id)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
            .bind(crate::lease::sql::LEASE_STATUS_REPORTED)
            .bind(self.succeeded)
            .bind(afd_billing::sql::work::FLEET_LEASE)
            .bind(af_wire::usage::LineKind::Model.as_str())
            .bind(afd_billing::RUN_NANOS_PER_SEC)
            .bind(self.runtime_price.as_ref())
            .bind(self.posture)
            .bind(self.provider)
            .bind(self.model)
            .bind(self.catalogue_version)
            .bind(self.line_ids)
            .bind(column(|line| i64::from(line.usage.seq)))
            .bind(
                lines
                    .iter()
                    .map(|line| line.usage.at.to_sqlx())
                    .collect::<Vec<_>>(),
            )
            .bind(column(|line| i64::from(line.usage.input_tokens)))
            .bind(column(|line| i64::from(line.usage.cached_input_tokens)))
            .bind(column(|line| i64::from(line.usage.cache_write_tokens)))
            .bind(column(|line| i64::from(line.usage.cache_write_5m_tokens)))
            .bind(column(|line| i64::from(line.usage.cache_write_1h_tokens)))
            .bind(column(|line| i64::from(line.usage.output_tokens)))
            .bind(
                lines
                    .iter()
                    .map(|line| line.price.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(column(|line| {
                i64::try_from(line.unpriced_tokens).unwrap_or(i64::MAX)
            }))
            .bind(column(|line| line.charged.as_i64()))
            .bind(self.runtime_line_id)
            .bind(self.status)
            .bind(self.response_text)
            .bind(self.tokens)
            .bind(self.wall_ms)
            .bind(self.failure_reason.as_deref())
            .bind(self.failure_detail)
            .bind(self.tool_calls)
            .bind(af_core::event::status::RECEIVED)
            .bind(afd_api_wire::approval::status::PENDING)
            .bind(self.checkpoint)
            .bind(self.held_until.map(jiff::Timestamp::to_sqlx))
            .bind(af_core::timing::SANDBOX_HOLD_IDLE)
            .bind(self.obligation_id)
            .bind(self.obligation_receipt)
            .bind(self.answer)
            .bind(
                afd_connector::Provider::ALL
                    .iter()
                    .map(|provider| provider.id())
                    .collect::<Vec<_>>(),
            )
            .bind(self.audit_id)
            .bind(afd_runner::sql::event_type::LEASE_RELEASED)
            .bind(sqlx::types::Json(&self.audit_metadata))
            .bind(self.fleet_id)
            .bind(self.workspace_id)
            .bind(i64::from(u32::MAX))
            .bind(af_wire::usage::LineKind::Runtime.as_str())
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))?;
        ReportAnswer::read(&row, self.lease_id)
    }
}

/// [`COMMIT_REPORT`]'s row, before the closing event is read from it.
#[derive(Debug, sqlx::FromRow)]
struct AnswerRow {
    claimed: i64,
    lease_status: Option<String>,
    held_posture: Option<String>,
    held_provider: Option<String>,
    held_model: Option<String>,
    catalogue_version: i64,
    fence_holds: Option<bool>,
    has_task: Option<bool>,
    reply_provider: Option<String>,
    owed_id: Option<Uuid7>,
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    output_tokens: Option<i64>,
    charged_nanos: i64,
    model_nanos: i64,
    event_id: Option<Uuid7>,
}

/// What the report statement answered.
#[derive(Debug)]
pub(crate) enum ReportAnswer {
    /// This report settled the lease, and every write landed.
    Settled(Box<SettledReport>),
    /// This runner settled the lease before; nothing was written.
    AlreadySettled,
    /// The lease's fence moved; nothing was written.
    Fenced,
    /// The lease is live under pricing facts other than the ones bound;
    /// nothing was written, and these are its own.
    Repriced(HeldFacts),
}

/// A settled report's answers.
#[derive(Debug)]
pub(crate) struct SettledReport {
    /// Whether the lease runs a durable task, whose usage and settlement
    /// follow in the same transaction.
    pub(crate) has_task: bool,
    pub(crate) closed: Option<Closed>,
    pub(crate) owed: Option<Uuid7>,
    pub(crate) reply_provider: Option<String>,
    pub(crate) tokens: RunTokens,
    pub(crate) charged: afd_billing::Nanos,
    pub(crate) model_nanos: afd_billing::Nanos,
}

/// The pricing facts a live lease holds, for the caller's one retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldFacts {
    pub(crate) posture: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) catalogue_version: i64,
}

impl ReportAnswer {
    fn read(row: &PgRow, lease_id: &Uuid7) -> Result<Self> {
        let answer = AnswerRow::from_row(row).map_err(query(CONTEXT))?;
        if answer.claimed == 0 {
            return Ok(Self::unclaimed(answer));
        }
        let closed = match answer.event_id {
            Some(_) => Some(Closed::read(row)?),
            None => None,
        };
        let tokens = tokens_of(&answer, lease_id);
        Ok(Self::Settled(Box::new(SettledReport {
            has_task: answer.has_task.unwrap_or(false),
            closed,
            owed: answer.owed_id,
            reply_provider: answer.reply_provider,
            tokens,
            charged: afd_billing::Nanos::from_i64(answer.charged_nanos),
            model_nanos: afd_billing::Nanos::from_i64(answer.model_nanos),
        })))
    }

    /// Why nothing was written: settled before, fenced, or priced wrongly.
    fn unclaimed(answer: AnswerRow) -> Self {
        if answer.lease_status.as_deref() == Some(crate::lease::sql::LEASE_STATUS_REPORTED) {
            return Self::AlreadySettled;
        }
        match (
            answer.fence_holds,
            answer.held_posture,
            answer.held_provider,
            answer.held_model,
        ) {
            (Some(true), Some(posture), Some(provider), Some(model)) => Self::Repriced(HeldFacts {
                posture,
                provider,
                model,
                catalogue_version: answer.catalogue_version,
            }),
            _ => Self::Fenced,
        }
    }
}

/// The run's token sums, or none, logged, when a sum does not fit: the
/// report's money has settled, so its metrics are what gives way.
fn tokens_of(answer: &AnswerRow, lease_id: &Uuid7) -> RunTokens {
    if let (
        Some(input_tokens),
        Some(cached_input_tokens),
        Some(cache_write_tokens),
        Some(output_tokens),
    ) = (
        answer.input_tokens,
        answer.cached_input_tokens,
        answer.cache_write_tokens,
        answer.output_tokens,
    ) {
        return RunTokens {
            input_tokens,
            cached_input_tokens,
            cache_write_tokens,
            output_tokens,
        };
    }
    let lease_id = lease_id.as_str();
    let error_code = af_core::error_code::INTERNAL_DB_QUERY.as_str();
    tracing::warn!(
        error_code,
        lease_id,
        reason = "a token sum does not fit in a signed 64-bit count",
        event = EVENT_RUN_TOKENS_UNREAD,
        "a settled run's tokens could not be summed; its metrics record none"
    );
    RunTokens::default()
}
