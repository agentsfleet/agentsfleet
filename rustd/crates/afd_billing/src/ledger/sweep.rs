//! The usage log read back: what a piece of work has cost so far, and the
//! lines that committed without a settlement.
//!
//! A work's own transaction logs its lines and a biller settles them after
//! commit (billing §11). A crash, an outage or a lost response between the two
//! leaves lines logged and unsettled; [`unsettled`] hands them back as the same
//! [`Settlement`]s the work would have sent, so a retry through any biller is
//! the original call again and `(work, seq)` keeps it from charging twice.

use std::collections::BTreeMap;
use std::time::Duration;

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
const SELECT_WORK_CHARGED: &str = include_str!("sql/select_work_charged.sql");
const SELECT_OWED: &str = include_str!("sql/select_owed.sql");

/// What each statement here was doing, for [`query`].
const CONTEXT_UNSETTLED: &str = "read unsettled usage lines";
const CONTEXT_CHARGED: &str = "read a work's charged usage";
const CONTEXT_OWED: &str = "read what is still owed";

/// How long a line waits before the sweeper settles it.
///
/// Long enough that the work's own post-commit settle has had its chance, so
/// the sweeper repairs failures rather than racing successes.
pub const GRACE: Duration = Duration::from_secs(120);

/// Lines one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// What one resettling pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resettled {
    /// Works whose unsettled lines this pass settled.
    pub settled: u64,
    /// Works it could not settle; they wait for the next pass.
    pub failed: u64,
}

/// Settles every work with lines logged more than [`GRACE`] ago and never
/// settled, through `biller`, and publishes what is still owed.
///
/// # Errors
/// The datastore would not answer a read. A biller refusing one settlement is
/// counted, not raised: the rest of the batch still runs, and the next pass
/// retries what failed.
pub async fn resettle(database: &afd_db::Db, biller: &dyn Biller) -> Result<Resettled> {
    let settlements = {
        let mut connection = database.acquire().await?;
        unsettled(&mut *connection, GRACE, BATCH_LIMIT).await?
    };
    let mut done = Resettled::default();
    for settlement in &settlements {
        match biller.settle(settlement).await {
            Ok(_receipt) => {
                done.settled += 1;
                metrics::sweeper_resettled(observed::work_type(&settlement.work), 1);
            }
            Err(_fault) => done.failed += 1,
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
    held_nanos: i64,
}

impl From<OwedRow> for metrics::Owed {
    fn from(row: OwedRow) -> Self {
        let gauge = |value: i64| u64::try_from(value).unwrap_or(0);
        Self {
            unsettled_nanos: [gauge(row.fleet_nanos), gauge(row.authoring_nanos)],
            oldest_seconds: gauge(row.oldest_seconds),
            held_nanos: gauge(row.held_nanos),
        }
    }
}

/// The table a malformed identifier is reported against.
const TABLE_LINES: &str = "billing.usage_lines";

/// What `work`'s logged lines cost in total, settled or not.
///
/// A renewal raises its hold to this plus the next slice's floor, so the hold
/// tracks what the work has actually used rather than how often it renewed.
///
/// # Errors
/// The statement would not run.
pub async fn charged_so_far<'e>(executor: impl PgExecutor<'e>, work: &WorkKey) -> Result<Nanos> {
    let charged: i64 = sqlx::query_scalar(SELECT_WORK_CHARGED)
        .bind(work.work_type())
        .bind(work.id().as_str())
        .fetch_one(executor)
        .await
        .map_err(query(CONTEXT_CHARGED))?;
    Ok(Nanos::from_i64(charged))
}

/// Up to `limit` lines logged more than `grace` ago and never settled, grouped
/// into one [`Settlement`] per work.
///
/// `grace` leaves the work's own post-commit settle its chance first; settling
/// is idempotent either way, so the grace only spares duplicate work.
///
/// # Errors
/// The statement would not run, or a stored identifier is not one.
pub async fn unsettled<'e>(
    executor: impl PgExecutor<'e>,
    grace: Duration,
    limit: i64,
) -> Result<Vec<Settlement>> {
    let rows: Vec<UnsettledRow> = sqlx::query_as(SELECT_UNSETTLED)
        .bind(grace)
        .bind(limit)
        .fetch_all(executor)
        .await
        .map_err(query(CONTEXT_UNSETTLED))?;
    let mut works: BTreeMap<(String, String), Settlement> = BTreeMap::new();
    for row in rows {
        let key = (row.work_type.clone(), row.work_id.clone());
        let Some(line) = row.line()? else {
            continue;
        };
        match works.get_mut(&key) {
            Some(settlement) => settlement.lines.push(line),
            None => {
                if let Some(settlement) = row.settlement(line)? {
                    works.insert(key, settlement);
                }
            }
        }
    }
    Ok(works.into_values().collect())
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
}

impl UnsettledRow {
    /// The row's line as the work priced it; `None` for a work type this build
    /// does not write, which no row can carry.
    fn line(&self) -> Result<Option<RatedLine>> {
        if self.work().is_none() {
            return Ok(None);
        }
        let price = self
            .price_id
            .as_deref()
            .map(|id| {
                Uuid7::parse(id).map_err(row_malformed(TABLE_LINES, "platform_model_price_id"))
            })
            .transpose()?;
        Ok(Some(RatedLine {
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
        }))
    }

    /// The work this row belongs to, by its stored kind.
    fn work(&self) -> Option<fn(Uuid7) -> WorkKey> {
        match self.work_type.as_str() {
            sql::work::FLEET_LEASE => Some(WorkKey::FleetLease),
            sql::work::AUTHORING_ATTEMPT => Some(WorkKey::AuthoringAttempt),
            _ => None,
        }
    }

    /// A settlement for this row's work, opening with `line`.
    fn settlement(self, line: RatedLine) -> Result<Option<Settlement>> {
        let Some(work) = self.work() else {
            return Ok(None);
        };
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
        Ok(Some(Settlement {
            work: work(id(&self.work_id, "work_id")?),
            payer: Account {
                tenant: id(&self.tenant_id, "tenant_id")?,
                workspace: id(&self.workspace_id, "workspace_id")?,
            },
            attribution,
            provider: self.provider,
            model: self.model,
            posture: Posture::parse(&self.posture).unwrap_or(Posture::Platform),
            lines: vec![line],
        }))
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
