//! The usage log read back: what a piece of work has cost so far, and the
//! lines that committed without a settlement.
//!
//! A work's own transaction logs its lines and returns; the billing relay
//! settles them (billing §11). [`unsettled`] hands them back as one
//! [`Settlement`] per work and pricing header, and [`resettle`] passes each to a biller, so a
//! biller in another process sees the same calls an in-process one does and
//! `(work, seq)` keeps a retried or repeated pass from charging twice.

use afd_core::error_code::ErrorCode;
use afd_core::id::Uuid7;
use afd_wire::usage::UsageLine;
use jiff::Timestamp;
use sqlx::PgExecutor;

use afd_observability::producers::billing as metrics;

use crate::biller::{Account, Attribution, Biller, Settlement, WorkKey, observed};
use crate::error::{query, row_malformed};
use crate::price::RatedLine;
use crate::rates::Posture;
use crate::{Nanos, Result, sql};

const SELECT_UNSETTLED: &str = include_str!("sql/select_unsettled.sql");
#[cfg(feature = "test-util")]
const SELECT_UNSETTLED_WORK: &str = include_str!("sql/select_unsettled_work.sql");
const SELECT_OWED: &str = include_str!("sql/select_owed.sql");

/// What each statement here was doing, for [`query`].
const CONTEXT_UNSETTLED: &str = "read unsettled usage lines";
const CONTEXT_OWED: &str = "read what is still owed";

/// Lines one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// Where the next pass resumes: just after the last line this one read.
///
/// The relay walks the unsettled lines oldest first in laps, so a work its
/// biller keeps refusing never holds every newer line behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resume {
    logged_at: Timestamp,
    line_id: String,
}

/// What one resettling pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resettled {
    /// Work/provider/model/posture groups this pass settled.
    pub settled: u64,
    /// Settlement groups it could not settle; they wait for the next pass.
    pub failed: u64,
    /// Whether the pass read a whole batch, so more lines may be waiting.
    pub full: bool,
    /// The code of the first refusal, for the pass's one aggregate record.
    pub refused: Option<ErrorCode>,
    /// Where the next pass resumes; `None` once a lap reached the newest line.
    pub resume: Option<Resume>,
}

/// Settles up to one batch of unsettled lines after `after` through `biller`,
/// one call per work/provider/model/posture group, and publishes what is still owed.
///
/// # Errors
/// The datastore would not answer a read. A biller refusing one settlement is
/// counted, not raised: the rest of the batch still runs, and the next pass
/// retries what failed.
pub async fn resettle(
    database: &afd_db::Db,
    biller: &dyn Biller,
    after: Option<&Resume>,
) -> Result<Resettled> {
    let (settlements, resume) = {
        let mut connection = database.acquire().await?;
        unsettled(&mut *connection, BATCH_LIMIT, after).await?
    };
    let mut done = Resettled {
        full: resume.is_some(),
        resume,
        ..Resettled::default()
    };
    for settlement in &settlements {
        match biller.settle(settlement).await {
            Ok(_receipt) => {
                done.settled += 1;
                metrics::sweeper_resettled(observed::work_type(&settlement.work), 1);
            }
            Err(fault) => {
                done.failed += 1;
                let error_code = fault.code();
                done.refused.get_or_insert(error_code);
                // Bound first: the macro repeats its field expressions.
                let (work_id, error_code) = (settlement.work.id().as_str(), error_code.as_str());
                tracing::debug!(
                    work_id,
                    error_code,
                    event = EVENT_REFUSED,
                    "the biller refused this work's settlement; the next pass retries"
                );
            }
        }
    }
    let owed = {
        let mut connection = database.acquire().await?;
        sqlx::query_as::<_, OwedRow>(SELECT_OWED)
            .bind(sql::work::FLEET_LEASE)
            .bind(sql::work::AUTHORING_ATTEMPT)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_OWED))
    };
    match owed {
        Ok(row) => metrics::owed_observed(Some(row.into())),
        Err(fault) => {
            metrics::owed_observed(None);
            return Err(fault);
        }
    }
    Ok(done)
}

/// What is still owed, by column name.
#[derive(Debug, sqlx::FromRow)]
struct OwedRow {
    fleet_nanos: i64,
    authoring_nanos: i64,
    oldest_seconds: i64,
}

impl From<OwedRow> for metrics::Owed {
    fn from(row: OwedRow) -> Self {
        let gauge = |value: i64| u64::try_from(value).unwrap_or(0);
        Self {
            unsettled_nanos: [gauge(row.fleet_nanos), gauge(row.authoring_nanos)],
            oldest_seconds: gauge(row.oldest_seconds),
        }
    }
}

/// The table a malformed identifier is reported against.
const TABLE_LINES: &str = "billing.usage_lines";

/// A work whose settlement the biller refused this pass; the next pass retries.
const EVENT_REFUSED: &str = "billing_relay_refused";

/// Up to `limit` unsettled lines after `after`, oldest first, grouped into
/// one [`Settlement`] per work, provider, model and pricing posture.
///
/// Answers where the next read resumes when this one filled its limit, and
/// `None` when it reached the newest line, so the next read starts a new lap.
///
/// # Errors
/// The statement would not run, or a stored identifier is not one.
pub async fn unsettled<'e>(
    executor: impl PgExecutor<'e>,
    limit: i64,
    after: Option<&Resume>,
) -> Result<(Vec<Settlement>, Option<Resume>)> {
    let rows: Vec<UnsettledRow> = sqlx::query_as(SELECT_UNSETTLED)
        .bind(limit)
        .bind(after.map(|resume| jiff_sqlx::Timestamp::from(resume.logged_at)))
        .bind(after.map(|resume| resume.line_id.as_str()))
        .fetch_all(executor)
        .await
        .map_err(query(CONTEXT_UNSETTLED))?;
    // Capture the oldest-first boundary before grouping by settlement header.
    let resume = (i64::try_from(rows.len()).unwrap_or(i64::MAX) >= limit)
        .then(|| rows.last())
        .flatten()
        .map(|last| Resume {
            logged_at: last.logged_at,
            line_id: last.line_id.clone(),
        });
    Ok((grouped(rows)?, resume))
}

/// `work`'s unsettled lines grouped by provider, model and pricing posture.
///
/// Uses the same grouping as the relay, scoped to one work so a test on a
/// shared database never settles another test's usage.
///
/// # Errors
/// The statement would not run, or a stored identifier is not one.
#[cfg(feature = "test-util")]
pub async fn unsettled_work<'e>(
    executor: impl PgExecutor<'e>,
    work: &WorkKey,
) -> Result<Vec<Settlement>> {
    let rows: Vec<UnsettledRow> = sqlx::query_as(SELECT_UNSETTLED_WORK)
        .bind(work.work_type())
        .bind(work.id().as_str())
        .fetch_all(executor)
        .await
        .map_err(query(CONTEXT_UNSETTLED))?;
    grouped(rows)
}

fn grouped(mut rows: Vec<UnsettledRow>) -> Result<Vec<Settlement>> {
    rows.sort_unstable_by(|left, right| {
        left.identity()
            .cmp(&right.identity())
            .then_with(|| left.seq.cmp(&right.seq))
    });
    let mut rows = rows.into_iter().peekable();
    let mut settlements = Vec::new();
    while let Some(row) = rows.next() {
        // Skip future work kinds without blocking the relay's cursor.
        let Some(work) = row.work() else {
            continue;
        };
        let mut lines = vec![row.line()?];
        while let Some(next) = rows.next_if(|next| next.identity() == row.identity()) {
            lines.push(next.line()?);
        }
        settlements.push(row.settlement(work, lines)?);
    }
    Ok(settlements)
}

/// One unsettled line, by column name.
#[derive(Debug, sqlx::FromRow)]
struct UnsettledRow {
    work_type: String,
    work_id: String,
    seq: i64,
    tenant_id: String,
    workspace_id: String,
    fleet_id: Option<String>,
    fleet_name: Option<String>,
    event_id: Option<String>,
    provider: String,
    model: String,
    posture: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    requested_at: Timestamp,
    input_token_count: i64,
    cached_input_token_count: i64,
    cache_write_token_count: i64,
    cache_write_5m_token_count: i64,
    cache_write_1h_token_count: i64,
    output_token_count: i64,
    runtime_ms: i64,
    price_id: Option<String>,
    unpriced_token_count: i64,
    charged_nanos: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    logged_at: Timestamp,
    line_id: String,
}

impl UnsettledRow {
    /// Money and attribution remain under the original work identity while
    /// independently configured providers retain their own settlement header.
    fn identity(&self) -> (&str, &str, &str, &str, &str) {
        (
            &self.work_type,
            &self.work_id,
            &self.provider,
            &self.model,
            &self.posture,
        )
    }

    /// The row's line as the work priced it.
    fn line(&self) -> Result<RatedLine> {
        let price = self
            .price_id
            .as_deref()
            .map(|id| {
                Uuid7::parse(id).map_err(row_malformed(TABLE_LINES, "platform_model_price_id"))
            })
            .transpose()?;
        Ok(RatedLine {
            usage: UsageLine {
                seq: count(self.seq),
                at: self.requested_at,
                input_tokens: count(self.input_token_count),
                cached_input_tokens: count(self.cached_input_token_count),
                cache_write_tokens: count(self.cache_write_token_count),
                cache_write_5m_tokens: count(self.cache_write_5m_token_count),
                cache_write_1h_tokens: count(self.cache_write_1h_token_count),
                output_tokens: count(self.output_token_count),
                runtime_ms: count(self.runtime_ms),
            },
            price,
            charged: Nanos::from_i64(self.charged_nanos),
            unpriced_tokens: u64::try_from(self.unpriced_token_count).unwrap_or(0),
        })
    }

    /// The work this row belongs to, by its stored kind.
    fn work(&self) -> Option<fn(Uuid7) -> WorkKey> {
        match self.work_type.as_str() {
            sql::work::FLEET_LEASE => Some(WorkKey::FleetLease),
            sql::work::AUTHORING_ATTEMPT => Some(WorkKey::AuthoringAttempt),
            _ => None,
        }
    }

    /// A settlement for this row's `work`, retaining its complete priced group.
    fn settlement(self, work: fn(Uuid7) -> WorkKey, lines: Vec<RatedLine>) -> Result<Settlement> {
        let id = |text: &str, column: &'static str| {
            Uuid7::parse(text).map_err(row_malformed(TABLE_LINES, column))
        };
        let attribution = match (self.fleet_id.as_deref(), self.event_id) {
            (Some(fleet), Some(event_id)) => Attribution::Fleet {
                fleet: id(fleet, "fleet_id")?,
                fleet_name: self.fleet_name.unwrap_or_default(),
                event_id,
            },
            _ => Attribution::SetupChat,
        };
        Ok(Settlement {
            work: work(id(&self.work_id, "work_id")?),
            payer: Account {
                tenant: id(&self.tenant_id, "tenant_id")?,
                workspace: id(&self.workspace_id, "workspace_id")?,
            },
            attribution,
            provider: self.provider,
            model: self.model,
            posture: Posture::parse(&self.posture).unwrap_or(Posture::Platform),
            lines,
        })
    }
}

/// A stored count as the line type holds it.
///
/// Every count was written from a `u32` and the table refuses negatives, so the
/// saturation never fires; it is here so a hand-edited row cannot panic a
/// sweeper.
fn count(stored: i64) -> u32 {
    u32::try_from(stored).unwrap_or(u32::MAX)
}
