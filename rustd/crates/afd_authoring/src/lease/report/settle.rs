//! Final settlement: the attempt's usage lines and its replay acknowledgement
//! commit together; money moves after commit, through the biller (billing §11).

use afd_core::clock;
use afd_wire::authoring::{Knowledge, Outcome, Report, Reported};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use super::{
    CONTEXT,
    prepare::Prepared,
    receipt, sql, usage,
    verdict::{self, Verdict},
};
use crate::{
    Drafts, Result,
    error::raise::query,
    funnel::Owed,
    lease::{Status, guard, meter},
    types::RUNNING,
};
use afd_billing::biller::Release;

pub(super) struct Settlement<'a> {
    pub(super) attempt: &'a guard::Attempt,
    pub(super) prepared: &'a Prepared,
    pub(super) verified: &'a meter::Verified,
    pub(super) verdict: &'a Verdict,
    pub(super) private: Option<&'a guard::Guarded>,
    pub(super) now: Timestamp,
    pub(super) acknowledgement: Acknowledgement,
}

/// Whether this report identity already holds a saved acknowledgement.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Acknowledgement {
    /// The first report under this identity: its acknowledgement is saved.
    First,
    /// A replay of a saved unsettled acknowledgement. The saved row stays
    /// immutable; the settlement row is what later replays read.
    Retained,
}

/// What a settled report owes once its transaction commits: the lines it
/// logged, to settle, and why the attempt's hold is released.
pub(super) struct Owing {
    pub(super) lines: afd_billing::biller::Settlement,
    pub(super) release: Release,
}

pub(super) async fn commit(
    drafts: &Drafts,
    connection: &mut PgConnection,
    settlement: Settlement<'_>,
    owed: &mut Owed,
) -> Result<(Reported, Option<Owing>)> {
    let known = settlement.verified.usage.knowledge == Knowledge::Known;
    let owing = if known {
        Some(log(drafts, connection, &settlement).await?)
    } else {
        None
    };
    update_attempt(connection, &settlement, known).await?;
    if known {
        reconcile(connection, &settlement).await?;
    }
    if let Some(private) = settlement.private {
        publish(drafts, connection, &settlement, private, owed).await?;
    }
    let retained = settlement.acknowledgement == Acknowledgement::Retained;
    let ack = Reported {
        settled: known,
        replayed: retained,
        sequence: settlement.attempt.message_sequence.to_string(),
        state: settlement.verdict.outcome,
    };
    if !retained {
        receipt::insert(
            drafts,
            connection,
            settlement.attempt,
            settlement.prepared,
            &ack,
            settlement.now,
        )
        .await?;
    }
    Ok((ack, owing))
}

async fn reconcile(connection: &mut PgConnection, s: &Settlement<'_>) -> Result<()> {
    use afd_wire::authoring::FailureReason;
    sqlx::query(sql::RESOLVED_REASON)
        .bind(s.attempt.draft_id.as_str())
        .bind(s.attempt.message_sequence)
        .bind(verdict::reason(
            s.verdict.reason.unwrap_or(FailureReason::ProviderFailed),
        ))
        .bind(verdict::reason(FailureReason::ProviderUsageUnknown))
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

/// Logs the attempt's priced usage lines inside the report's transaction.
///
/// Usage the daemon knows is what settles: one line per known dispatch and
/// one for the runtime, logged once per attempt (`(work, seq)` is unique, so a
/// replay that reaches here again logs nothing new).
async fn log(drafts: &Drafts, connection: &mut PgConnection, s: &Settlement<'_>) -> Result<Owing> {
    let lines = usage::settlement(connection, s.attempt, s.verified, s.now).await?;
    afd_billing::ledger::log_lines(&mut *connection, &drafts.entropy, s.now, &lines).await?;
    let release = if s.verdict.outcome == Outcome::Completed {
        Release::Completed
    } else {
        Release::Failed
    };
    Ok(Owing { lines, release })
}

async fn update_attempt(
    connection: &mut PgConnection,
    s: &Settlement<'_>,
    known: bool,
) -> Result<()> {
    sqlx::query(sql::ATTEMPT)
        .bind(s.attempt.id.as_str())
        .bind(
            if known {
                Status::Settled
            } else {
                Status::AwaitingAccounting
            }
            .as_str(),
        )
        .bind(!known)
        .bind(clock::saturating_millis_signed(s.verified.runtime))
        .bind(s.now.to_sqlx())
        .bind(known.then(|| verdict::outcome(s.verdict.outcome)))
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

async fn publish(
    drafts: &Drafts,
    connection: &mut PgConnection,
    s: &Settlement<'_>,
    private: &guard::Guarded,
    owed: &mut Owed,
) -> Result<()> {
    write_message(connection, s, private).await?;
    owed.finished(
        connection,
        s.attempt.into(),
        verdict::outcome(s.verdict.outcome),
    )
    .await?;
    // Only a terminal report publishes, and every terminal report carries an
    // answer digest, absent answer included.
    if let (true, Some(digest)) = (s.verdict.answer_allowed, &s.prepared.answer_digest) {
        receipt::answer(drafts, connection, s.attempt, s.prepared, digest, s.now).await?;
    }
    Ok(())
}

/// Writes the verdict onto the still-running message: this report's finish.
///
/// Only a report holding private access gets here, and `guard::bound` proved
/// the message running under the attempt's row lock, which every writer that
/// finishes a message takes first; so this write is the one that finishes it.
async fn write_message(
    connection: &mut PgConnection,
    s: &Settlement<'_>,
    private: &guard::Guarded,
) -> Result<()> {
    let answer = match &*s.prepared.request {
        Report::Terminal { answer, .. }
            if s.verdict.answer_allowed && s.verdict.outcome == Outcome::Completed =>
        {
            answer.as_deref()
        }
        _ => None,
    };
    sqlx::query(sql::MESSAGE)
        .bind(s.attempt.draft_id.as_str())
        .bind(s.attempt.message_sequence)
        .bind(verdict::outcome(s.verdict.outcome))
        .bind(answer)
        .bind(
            private
                .draft
                .head_candidate_id
                .as_ref()
                .map(afd_core::id::Uuid7::as_str),
        )
        .bind(s.verdict.reason.map(verdict::reason))
        .bind(RUNNING)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
