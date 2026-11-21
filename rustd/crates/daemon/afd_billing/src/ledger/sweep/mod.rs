//! The usage log read back: a work's lines that committed without a
//! settlement, grouped into the settlements a biller is asked to charge.
//!
//! A work's own transaction logs its lines and returns. No relay settles them
//! any more (Indy, Nov 21, 2026); [`unsettled_work`] lets a test read what a
//! work still owes.

use af_wire::usage::LineKind;
use sqlx::PgExecutor;

use crate::biller::{Settlement, WorkKey};
use crate::error::query;
use crate::rates::Posture;
use crate::Result;

mod row;

use self::row::UnsettledRow;
use super::unreadable::{Unreadable, skipped};

const SELECT_UNSETTLED_WORK: &str = include_str!("../sql/select_unsettled_work.sql");

/// What the statement here was doing, for [`query`].
const CONTEXT_UNSETTLED: &str = "read unsettled usage lines";

/// `work`'s unsettled lines grouped by provider, model and pricing posture,
/// scoped to one work so a test on a shared database never reads another
/// test's usage.
///
/// # Errors
/// The statement would not run.
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
        // Skip what this build cannot read, without guessing: the group stays
        // unsettled and is named once.
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
        let work_type = row.work_type.clone();
        let Some(settlement) = row.settlement(work, posture, lines) else {
            skipped(Unreadable::Attribution, &work_type, &work_type);
            continue;
        };
        settlements.push(settlement);
    }
    settlements
}

#[cfg(test)]
mod tests;
