//! The writes a won report owes, and which side of the commit each falls on.
//!
//! The terminal event row, the session checkpoint, the stream acknowledgement
//! and the audit row that closes the lease's history. The Zig ran all of them
//! after the money, independently, each logged if it did not land — five
//! separate facts about a run already paid for. That shape is kept for the two
//! that cannot be anything else and abandoned for the two that can.
//!
//! # Two of these are the report, and two are about it
//!
//! [`Leases::mark_terminal`] and [`Leases::checkpoint`] take the connection
//! their caller is already inside, because the RESULT of a run and the money
//! charged for it are one fact: a settle that commits without its result
//! leaves a tenant charged for a run whose answer is nowhere, and no retry
//! recovers it — the lease is `reported`, so the retry is refused. They commit
//! with the settle in [`Leases::commit_report`](crate::lease::commit) or
//! neither does.
//!
//! [`Leases::acknowledge`] and [`Leases::record_released`] stay outside it, and
//! not because they matter less. The acknowledgement is a QUEUE write, and no
//! transaction spans Postgres and Dragonfly; putting it inside would mean
//! acknowledging an entry a rollback then un-did, which is the one ordering
//! that loses work outright. It therefore runs after the commit, where a
//! failure leaves the entry pending and re-delivered rather than a result
//! unrecoverable. The audit row is history — a datastore blip writing it must
//! not fail a report whose money has committed.
//!
//! Both post-commit writes are attempted, logged on failure, and never
//! propagated: by then the lease is `reported` and the wallet is drawn down, so
//! failing the response would tell the runner to retry a report whose money
//! cannot be charged twice. What an operator gets is a warn line naming which
//! one did not land.
//!
//! # The cap is `is_char_boundary`, not a nibble walk
//!
//! `event_rows.truncateUtf8` walks back over continuation bytes by masking
//! `0xC0`, because Zig's standard library gave it nothing better. Rust has
//! [`str::is_char_boundary`], which asks the question directly. See
//! [`truncate`].

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_dragonfly::EventId;
use afd_events::Closed;

use sqlx::PgConnection;

use crate::error::Result;
use crate::lease::sql;
use crate::lease::sql::session::MAX_CHECKPOINT_RESPONSE_BYTES;
use crate::lease::store::Leases;
use crate::lease::verdict::{Terminal, truncate};

/// Statement name, for the context a query failure carries.
const CONTEXT_TERMINAL: &str = "fleet event terminal";

/// Statement name, for the context a query failure carries.
const CONTEXT_CHECKPOINT: &str = "fleet session checkpoint";

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASED: &str = "runner lease released event";

/// The session cursor a fleet resumes from.
///
/// Serialized through `serde` rather than assembled by hand: the Zig builds an
/// anonymous struct and stringifies it, which is the same thing, and the
/// failure arm — `catch "{}"` — is what a `Result` here says out loud instead.
#[derive(Debug, serde::Serialize)]
struct Checkpoint<'a> {
    last_event_id: &'a str,
    last_response: &'a str,
}

impl Leases {
    /// End the event with the runner's verdict.
    ///
    /// Guarded on the row still being `received`, so a terminal row is never
    /// reopened and a redelivery whose acknowledgement was lost cannot
    /// overwrite the settled result. No row is that case, and it is logged
    /// rather than treated as a failure. The row that DID close comes back
    /// with the fleet facts beside it, for the completion frame the caller
    /// announces on the live tail — after the commit, because a frame
    /// announcing an ending a rollback then removed is worse than a late one.
    ///
    /// Runs on the caller's connection: this is the RESULT half of the one
    /// transaction the settle rides, per the module note.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a closing this daemon
    /// cannot read.
    pub async fn mark_terminal(
        &self,
        connection: &mut PgConnection,
        fleet_id: &Uuid7,
        event_id: &str,
        outcome: Terminal<'_>,
        now: UnixMillis,
    ) -> Result<Option<Closed>> {
        let Terminal {
            verdict,
            response_text,
            tokens,
            wall_ms,
        } = outcome;
        let closed = sqlx::query(afd_events::sql::UPDATE_FLEET_EVENT_RESULT)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .bind(verdict.status())
            .bind(response_text)
            .bind(tokens)
            .bind(wall_ms)
            .bind(now.as_millis())
            .bind(verdict.label())
            .bind(afd_core::event::status::RECEIVED)
            .bind(verdict.detail())
            .bind(afd_wire::approval::status::PENDING)
            .fetch_optional(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_TERMINAL))?;

        let Some(row) = closed else {
            let fleet = fleet_id.as_str();
            tracing::warn!(
                fleet_id = fleet,
                agentsfleet_event_id = event_id,
                event = "terminal_write_skipped_nonreceived",
                "the event was already terminal; the settled result stands"
            );
            return Ok(None);
        };
        Ok(Some(Closed::read(&row)?))
    }

    /// Record where this fleet's session resumes.
    ///
    /// Runs on the caller's connection, inside the settle's transaction: a
    /// session left pointing at the run before this one would re-feed the
    /// previous answer to a run that has already been paid for.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a cursor that will not
    /// serialize — which cannot happen for two string fields, and is reported
    /// rather than swallowed so that stays true by test rather than by belief.
    pub async fn checkpoint(
        &self,
        connection: &mut PgConnection,
        fleet_id: &Uuid7,
        last_event_id: &str,
        last_response: &str,
        now: UnixMillis,
    ) -> Result<()> {
        // `Value`, then `Display`. `serde_json::to_string` is fallible in its
        // signature and cannot fail for two string fields, which would leave an
        // error arm no test could reach and no caller could act on — the Zig
        // spells the same dead branch as `catch "{}"`, silently checkpointing a
        // fleet to nothing. Rendering a `Value` has no failure to absorb.
        let document = serde_json::json!(Checkpoint {
            last_event_id,
            last_response: truncate(last_response, MAX_CHECKPOINT_RESPONSE_BYTES),
        })
        .to_string();

        sqlx::query(sql::session::UPSERT_FLEET_SESSION)
            .bind(fleet_id.as_str())
            .bind(&document)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_CHECKPOINT))?;
        Ok(())
    }

    /// Acknowledge the stream entry this lease executed.
    ///
    /// Takes the RECEIPT, never the logical event id: `XACK` addresses the
    /// entry, and a replayed admission puts one logical event on two of them.
    /// Passing the logical id would acknowledge nothing and leave the entry
    /// pending forever.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    pub async fn acknowledge(&self, fleet_id: &Uuid7, receipt: &EventId) -> Result<()> {
        let fleet = fleet_id.as_str();
        let acknowledged = self.streams().ack(fleet, receipt).await?;
        if !acknowledged {
            let entry = receipt.as_str();
            tracing::warn!(
                fleet_id = fleet,
                receipt = entry,
                event = "xack_no_entry",
                "the stream entry was already acknowledged or trimmed"
            );
        }
        self.trim_history(fleet).await;
        Ok(())
    }

    /// Trims the fleet's acknowledged history, now that it has grown by one.
    ///
    /// Best-effort like every write in this module: the acknowledgement
    /// already landed, and a trim that did not is retried by the next one.
    /// Per-acknowledgement, so at `debug`; a trim that fails is `warn`,
    /// because a stream that is never trimmed grows until the admission
    /// budget refuses its producers.
    async fn trim_history(&self, fleet: &str) {
        match self.streams().trim(fleet).await {
            Ok(trimmed) if trimmed.removed > 0 => {
                let removed = trimmed.removed;
                let retained = trimmed.retained;
                tracing::debug!(
                    fleet_id = fleet,
                    removed,
                    retained,
                    event = "stream_history_trimmed"
                );
            }
            Ok(_nothing_above_the_floor) => {}
            Err(failure) => {
                let code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                let reason = failure.to_string();
                tracing::warn!(
                    error_code = code,
                    fleet_id = fleet,
                    reason,
                    event = "stream_trim_failed"
                );
            }
        }
    }

    /// Close the lease's history with the row that pairs its acquisition.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the row's identifier,
    /// an instant that cannot be encoded, and a datastore that would not
    /// answer.
    pub async fn record_released(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        fleet_id: &Uuid7,
        event_id: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let row_id = Uuid7::encode(now, bytes)?;

        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::report::INSERT_RUNNER_EVENT)
            .bind(row_id.as_str())
            .bind(runner_id.as_str())
            .bind(afd_runner::sql::event_type::LEASE_RELEASED)
            .bind(now.as_millis())
            .bind(afd_runner::sql::meta::LEASE_ID)
            .bind(lease_id)
            .bind(afd_runner::sql::meta::FLEET_ID)
            .bind(fleet_id.as_str())
            .bind(afd_runner::sql::meta::AGENTSFLEET_EVENT_ID)
            .bind(event_id)
            .execute(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_RELEASED))?;
        Ok(())
    }
}
