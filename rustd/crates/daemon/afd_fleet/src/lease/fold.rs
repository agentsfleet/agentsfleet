//! A live run folding in the messages a person sends in its conversation
//! while it works: the claim before each model call, and the settle its
//! report makes for every message a completed call read.
//!
//! A folded message is answered the way a durable task's folded input is
//! (`task::input_receipt::answer`): its own event opened, then closed with the
//! run's answer. It is in the run's conversation, so the run's own reply
//! reaches its thread; it owes none of its own.

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::FoldOutcome;
use af_observability::producers;
use af_wire::fold::{FOLDS_PER_CLAIM_MAX, FoldRequest, FoldResponse, FoldedInput};
use sqlx::PgConnection;

use afd_events::Closed;

use crate::error::{Result, query};
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;
use crate::lease::sql;
use crate::lease::store::Leases;
use crate::lease::verdict::{Terminal, Verdict};

/// The context a claim that failed carries.
const CONTEXT_CLAIM: &str = "claim a run's folded messages";
/// The context a settle that failed carries.
const CONTEXT_SETTLE: &str = "settle a run's folded messages";
/// The context an event that would not open carries.
const CONTEXT_OPEN: &str = "open the folded messages' events before completing them";
/// A claim answered a run: per model call, so at `debug` (LOG-HOT).
const EVENT_CLAIMED: &str = "folds_claimed";
/// A committed report settled the messages its run folded and read.
const EVENT_SETTLED: &str = "folds_settled";

/// One claimed message, as the claim reads it.
#[derive(Debug, sqlx::FromRow)]
struct ClaimedRow {
    id: String,
    sequence: i64,
    request_json: String,
}

/// What a report's folds leave its caller with: each message's closed event,
/// for the live tail.
///
/// Carries the facts the settle logs and counts, because the settle runs
/// inside the report's transaction and a rollback must leave no record of it.
/// [`Folds::record_settled`] writes them once the report has committed.
#[derive(Debug, Default)]
pub struct Folds {
    /// Each folded message's event, closed with the run's answer.
    pub closed: Vec<Closed>,
    /// The last sequence the run's completed model calls read.
    pub read_through: i64,
}

impl Folds {
    /// Logs and counts the messages a report that has committed settled.
    ///
    /// Called only after the commit: a failed commit rolls the settle back, and
    /// a "settled" record for rows that are runnable again would be false.
    pub(crate) fn record_settled(&self, fleet_id: &str, lease_id: &str) {
        if self.closed.is_empty() {
            return;
        }
        let (settled, read_through) = (self.closed.len(), self.read_through);
        let event = EVENT_SETTLED;
        tracing::info!(fleet_id, lease_id, read_through, settled, event);
        producers::fleet::fold::folds(
            FoldOutcome::Settled,
            u64::try_from(settled).unwrap_or(u64::MAX),
        );
    }
}

impl Plane {
    /// The messages `lease_id`'s fleet received after `request.after_sequence`,
    /// claimed for the run, oldest first. See [`Leases::claim_folds`].
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn folds(
        &self,
        runner_id: &Uuid7,
        lease_id: &Uuid7,
        request: &FoldRequest,
    ) -> Result<FoldResponse<'static>> {
        self.leases
            .claim_folds(
                runner_id,
                lease_id,
                request.fencing_token,
                request.after_sequence,
            )
            .await
    }
}

impl Leases {
    /// The messages `lease_id`'s fleet received after `after_sequence`,
    /// claimed for the run under `fencing_token`, oldest first.
    ///
    /// A lease that is not this runner's, not live, or superseded claims
    /// nothing, which reads as no message: the run learns it lost the lease
    /// from its renewal, as it would without this call.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn claim_folds(
        &self,
        runner_id: &Uuid7,
        lease_id: &Uuid7,
        fencing_token: u64,
        after_sequence: i64,
    ) -> Result<FoldResponse<'static>> {
        let fence = i64::try_from(fencing_token).unwrap_or(i64::MAX);
        let mut connection = self.pool().acquire().await?;
        let rows: Vec<ClaimedRow> = sqlx::query_as(sql::lease::CLAIM_INPUTS)
            .bind(lease_id.as_str())
            .bind(runner_id.as_str())
            .bind(fence)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(after_sequence)
            .bind(i64::from(FOLDS_PER_CLAIM_MAX))
            .bind(afd_admission::Producer::Steer.as_str())
            .bind(afd_events::ACTOR_MACHINE)
            .bind(format!("{}%", afd_events::ACTOR_PREFIX))
            .bind(afd_admission::Producer::SlackMention.as_str())
            .bind(afd_connector::Provider::Slack.id())
            .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
            .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLAIM))?;
        let (lease, claimed, event) = (lease_id.as_str(), rows.len(), EVENT_CLAIMED);
        tracing::debug!(lease_id = lease, after_sequence, claimed, event);
        producers::fleet::fold::folds(
            FoldOutcome::Claimed,
            u64::try_from(claimed).unwrap_or(u64::MAX),
        );
        Ok(FoldResponse {
            inputs: rows
                .into_iter()
                .map(|row| FoldedInput {
                    text: af_wire::event::message_of(&row.request_json)
                        .into_owned()
                        .into(),
                    event_id: row.id.into(),
                    sequence: row.sequence,
                })
                .collect(),
        })
    }

    /// Settles the messages `lease` claimed and a completed model call read,
    /// through `read_through`, inside the report's transaction, and gives
    /// each its own event closed with `answer`: three statements however many
    /// messages there are. A run that read nothing (`None`) settles nothing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, in which case the report's
    /// transaction writes nothing.
    pub(crate) async fn settle_folds(
        &self,
        connection: &mut PgConnection,
        lease: &Reported,
        lease_id: &str,
        read_through: Option<i64>,
        answer: &str,
    ) -> Result<Folds> {
        let Some(read_through) = read_through else {
            return Ok(Folds::default());
        };
        let settled: Vec<String> = sqlx::query_scalar(sql::lease::SETTLE_FOLDS)
            .bind(lease.fleet_id.as_str())
            .bind(lease_id)
            .bind(lease.fence.as_i64())
            .bind(read_through)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_SETTLE))?;
        if settled.is_empty() {
            return Ok(Folds {
                read_through,
                ..Folds::default()
            });
        }
        sqlx::query(afd_events::sql::OPEN_FOLDED_EVENTS)
            .bind(lease.fleet_id.as_str())
            .bind(&settled)
            .bind(&lease.event_id)
            .bind(af_core::event::status::RECEIVED)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_OPEN))?;
        let closed = self
            .mark_terminal_each(
                connection,
                &lease.fleet_id,
                &settled,
                Terminal {
                    verdict: Verdict::Succeeded,
                    response_text: answer,
                    tokens: 0,
                    wall_ms: 0,
                    tool_calls: None,
                },
            )
            .await?;
        Ok(Folds {
            closed,
            read_through,
        })
    }
}
