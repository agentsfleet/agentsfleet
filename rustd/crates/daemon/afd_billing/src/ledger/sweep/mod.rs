//! The usage log read back: what a piece of work has cost so far, and the
//! lines that committed without a settlement.
//!
//! A work's own transaction logs its lines and returns; the billing relay
//! settles them (billing §11). [`unsettled`] hands them back as one
//! [`Settlement`] per work and pricing header, and [`resettle`] passes each to a biller, so a
//! biller in another process sees the same calls an in-process one does and
//! `(work, kind, seq)` keeps a retried or repeated pass from charging twice.

use af_core::error_code::ErrorCode;
use af_core::id::Uuid7;
use af_wire::usage::LineKind;
use jiff::Timestamp;
use sqlx::PgExecutor;

use af_observability::producers::billing as metrics;

#[cfg(feature = "test-util")]
use crate::biller::WorkKey;
use crate::biller::{Biller, Settlement, observed};
use crate::error::query;
use crate::rates::Posture;
use crate::{Result, sql};

mod row;

use self::row::UnsettledRow;
use super::unreadable::{Unreadable, skipped};

const SELECT_UNSETTLED: &str = include_str!("../sql/select_unsettled.sql");
#[cfg(feature = "test-util")]
const SELECT_UNSETTLED_WORK: &str = include_str!("../sql/select_unsettled_work.sql");
const SELECT_OWED: &str = include_str!("../sql/select_owed.sql");

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
    line_id: Uuid7,
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

/// A work whose settlement the biller refused this pass; the next pass retries.
const EVENT_REFUSED: &str = "billing_relay_refused";

/// Up to `limit` unsettled lines after `after`, oldest first, grouped into
/// one [`Settlement`] per work, provider, model and pricing posture.
///
/// Answers where the next read resumes when this one filled its limit, and
/// `None` when it reached the newest line, so the next read starts a new lap.
///
/// A group whose work type, line kind or posture this build cannot name is left
/// out and stays unsettled; see [`super::unreadable`].
///
/// # Errors
/// The statement would not run.
pub async fn unsettled<'e>(
    executor: impl PgExecutor<'e>,
    limit: i64,
    after: Option<&Resume>,
) -> Result<(Vec<Settlement>, Option<Resume>)> {
    let rows: Vec<UnsettledRow> = sqlx::query_as(SELECT_UNSETTLED)
        .bind(limit)
        .bind(after.map(|resume| jiff_sqlx::Timestamp::from(resume.logged_at)))
        .bind(after.map(|resume| &resume.line_id))
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
    Ok((grouped(rows), resume))
}

/// `work`'s unsettled lines grouped by provider, model and pricing posture.
///
/// Uses the same grouping as the relay, scoped to one work so a test on a
/// shared database never settles another test's usage.
///
/// # Errors
/// The statement would not run.
#[cfg(feature = "test-util")]
pub async fn unsettled_work<'e>(
    executor: impl PgExecutor<'e>,
    work: &WorkKey,
) -> Result<Vec<Settlement>> {
    let rows: Vec<UnsettledRow> = sqlx::query_as(SELECT_UNSETTLED_WORK)
        .bind(work.work_type())
        .bind(work.id())
        .fetch_all(executor)
        .await
        .map_err(query(CONTEXT_UNSETTLED))?;
    Ok(grouped(rows))
}

fn grouped(mut rows: Vec<UnsettledRow>) -> Vec<Settlement> {
    rows.sort_unstable_by(|left, right| {
        left.identity()
            .cmp(&right.identity())
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.seq.cmp(&right.seq))
    });
    let mut rows = rows.into_iter().peekable();
    let mut settlements = Vec::new();
    while let Some(row) = rows.next() {
        let mut group = Vec::new();
        while let Some(next) = rows.next_if(|next| next.identity() == row.identity()) {
            group.push(next);
        }
        // Skip what this build cannot read, without blocking the relay's cursor
        // and without guessing: the group stays unsettled and is named once.
        let Some(work) = row.work() else {
            skipped(Unreadable::WorkType, &row.work_type, &row.work_type);
            continue;
        };
        let Some(posture) = Posture::parse(&row.posture) else {
            skipped(Unreadable::Posture, &row.work_type, &row.posture);
            continue;
        };
        let Some(lines) = std::iter::once(&row)
            .chain(&group)
            .map(UnsettledRow::line)
            .collect::<Option<Vec<_>>>()
        else {
            let unnamed = std::iter::once(&row)
                .chain(&group)
                .find(|line| LineKind::parse(&line.kind).is_none())
                .map_or(row.kind.as_str(), |line| line.kind.as_str());
            skipped(Unreadable::LineKind, &row.work_type, unnamed);
            continue;
        };
        settlements.push(row.settlement(work, posture, lines));
    }
    settlements
}

#[cfg(test)]
mod tests;
