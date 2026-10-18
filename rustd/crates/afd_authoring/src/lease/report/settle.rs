//! Final settlement, actual wallet debit and replay acknowledgement commit together.

use afd_core::clock;
use afd_wire::authoring::{Knowledge, Outcome, Report, Reported};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use super::{
    CONTEXT,
    prepare::Prepared,
    receipt, sql,
    verdict::{self, Verdict},
};
use crate::{
    Drafts, Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    funnel::Owed,
    lease::{Status, guard, meter},
    types::RUNNING,
};

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

pub(super) async fn commit(
    drafts: &Drafts,
    connection: &mut PgConnection,
    settlement: Settlement<'_>,
    owed: &mut Owed,
) -> Result<Reported> {
    let known = settlement.verified.usage.knowledge == Knowledge::Known;
    if known {
        debit(drafts, connection, &settlement).await?;
    }
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
    Ok(ack)
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

async fn debit(drafts: &Drafts, connection: &mut PgConnection, s: &Settlement<'_>) -> Result<()> {
    let provider = if s.attempt.model_snapshot.platform_managed {
        s.verified.provider_nanos
    } else {
        0
    };
    let due = s
        .verified
        .runtime_nanos
        .checked_add(provider)
        .ok_or(ErrorKind::Balance)?;
    let charged: i64 = sqlx::query_scalar(sql::DEBIT)
        .bind(s.attempt.tenant_id.as_str())
        .bind(due)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(ErrorKind::Balance)?;
    let usage = serde_json::json!({"knowledge":"known","calls":s.verified.calls,
        "input_tokens":s.verified.usage.input_tokens,"output_tokens":s.verified.usage.output_tokens,
        "runtime_ms":clock::saturating_millis_signed(s.verified.runtime),
        "provider_cost_nanos":s.verified.provider_nanos.to_string()});
    sqlx::query(sql::SETTLEMENT)
        .bind(drafts.mint(s.now)?.as_str())
        .bind(s.attempt.id.as_str())
        .bind(s.prepared.report_id.as_str())
        .bind(&s.prepared.digest)
        .bind(verdict::outcome(s.verdict.outcome))
        .bind(s.verdict.reason.map(verdict::reason))
        .bind(serde_json::to_string(&usage).map_err(stored)?)
        .bind(charged)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
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
    let state = verdict::outcome(s.verdict.outcome);
    if write_message(connection, s, private, false).await? {
        owed.finished(connection, s.attempt.into(), state).await?;
    } else {
        write_message(connection, s, private, true).await?;
    }
    if s.verdict.answer_allowed {
        receipt::answer(drafts, connection, s.attempt, s.prepared, s.now).await?;
    }
    Ok(())
}

/// Writes the verdict onto the message, as its first finish or as the late
/// rewrite of an unknown-usage failure; whether it matched.
async fn write_message(
    connection: &mut PgConnection,
    s: &Settlement<'_>,
    private: &guard::Guarded,
    late: bool,
) -> Result<bool> {
    let answer = match &*s.prepared.request {
        Report::Terminal { answer, .. }
            if s.verdict.answer_allowed && s.verdict.outcome == Outcome::Completed =>
        {
            answer.as_deref()
        }
        _ => None,
    };
    let statement = if late {
        sql::LATE_MESSAGE
    } else {
        sql::MESSAGE
    };
    let mut write = sqlx::query(statement)
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
        .bind(RUNNING);
    if late {
        write = write.bind(verdict::reason(
            afd_wire::authoring::FailureReason::ProviderUsageUnknown,
        ));
    }
    let written = write.execute(connection).await.map_err(query(CONTEXT))?;
    Ok(written.rows_affected() == 1)
}
