//! The activity verb: live-tail frames a runner forwards while its child works.
//!
//! A runner holds no Redis, so it ships progress frames here and the daemon
//! publishes them to `fleet:{id}:activity` for the dashboard's live tail.
//!
//! # Best-effort, and what that actually licenses
//!
//! A dropped frame is cosmetic — the durable record is the report — so a
//! publish that fails is logged and the verb still answers. What is NOT
//! best-effort is authorization: the lease must resolve and belong to the
//! presenting runner, because without that check a runner could publish onto a
//! fleet it holds no lease on and write into somebody else's live tail.
//!
//! No fencing, deliberately. A superseded holder's cosmetic frames are
//! harmless, and the tail is never a source of truth — which is why the load
//! below has no `status` predicate either.
//!
//! # The vocabulary bridge
//!
//! This is the one seam where the runner's frame names become the dashboard's.
//! They are NOT the same vocabulary: `fleet_response_chunk` on the wire is
//! `chunk` on the channel, and that single rename is the whole reason a
//! translation type exists rather than the wire frame being re-serialized. The
//! `From` below is total over [`ActivityFrame`], so a new frame variant fails
//! the build until somebody decides what the dashboard calls it.

use afd_core::id::Uuid7;
use afd_redis::fleet_activity_channel;
use afd_wire::activity::ActivityFrame;
use serde::Serialize;
use serde_json::value::RawValue;
use sqlx::Row as _;

use crate::error::{Result, query, row_malformed};
use crate::lease::store::Leases;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_TARGET: &str = "activity lease load";

/// A frame could not be published; the tail loses it and the run does not care.
const EVENT_DROPPED: &str = "activity_frame_dropped";

/// What one publish needs: whose channel, and which event the frames belong to.
#[derive(Debug, Clone)]
pub struct Target {
    /// The fleet whose channel carries the tail.
    pub fleet_id: Uuid7,
    /// The event the frames describe.
    pub event_id: String,
}

/// One frame as the dashboard reads it.
///
/// Tagged by `kind`, which is the discriminator `events.ts` switches on. The
/// payload field names are the Zig's, because the consumer is unchanged.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Published<'a> {
    ToolCallStarted {
        event_id: &'a str,
        name: &'a str,
        /// Spliced in verbatim, NOT re-encoded — see [`Published::of`].
        args_redacted: &'a RawValue,
    },
    ToolCallProgress {
        event_id: &'a str,
        name: &'a str,
        elapsed_ms: i64,
    },
    /// The one rename in the bridge: `fleet_response_chunk` on the wire.
    #[serde(rename = "chunk")]
    Chunk { event_id: &'a str, text: &'a str },
    ToolCallCompleted {
        event_id: &'a str,
        name: &'a str,
        ms: i64,
    },
}

impl Leases {
    /// Publish one batch of frames to `target`'s channel.
    ///
    /// Never fails the verb. Each frame is published independently so one
    /// unencodable frame does not silence the rest of the batch, and a Redis
    /// outage costs the tail rather than the run.
    pub async fn publish_activity(&self, target: &Target, frames: &[ActivityFrame<'_>]) {
        let channel = fleet_activity_channel(target.fleet_id.as_str());
        let streams = self.streams();
        for frame in frames {
            // `args_redacted` arrives as a STRING holding JSON, and the Zig
            // parses it into a `std.json.Value` purely to splice it back out
            // again — a whole tree built and dropped per tool call, because Zig
            // has no way to say "these bytes are already JSON". `RawValue` says
            // exactly that: it validates the syntax and keeps the bytes, so the
            // frame is checked without ever being materialised.
            let owned;
            let published = match Published::of(target, frame) {
                Ok(value) => {
                    owned = value;
                    &owned
                }
                Err(malformed) => {
                    let fleet = target.fleet_id.as_str();
                    tracing::debug!(
                        fleet_id = fleet,
                        reason = %malformed,
                        event = EVENT_DROPPED,
                        "a frame carried arguments that are not JSON; the tail loses it"
                    );
                    continue;
                }
            };
            let Ok(payload) = serde_json::to_string(published) else {
                continue;
            };
            if let Err(unreachable_queue) = streams.publish(&channel, &payload).await {
                let fleet = target.fleet_id.as_str();
                let reason = unreachable_queue.to_string();
                tracing::debug!(
                    fleet_id = fleet,
                    reason,
                    event = EVENT_DROPPED,
                    "the queue would not take a live-tail frame; the run is unaffected"
                );
            }
        }
    }

    /// The lease `lease_id` names, if it belongs to `runner_id`.
    ///
    /// No `status` predicate: an expired lease still resolves, because a
    /// superseded holder's cosmetic frames are harmless and refusing them would
    /// cut the tail off exactly when a run is being reclaimed — the moment an
    /// operator is most likely to be watching it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a `fleet_id` that is not
    /// an identifier.
    pub async fn load_activity_target(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
    ) -> Result<Option<Target>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::activity::SELECT_LEASE_TARGET)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TARGET))?;

        let Some(row) = found else {
            return Ok(None);
        };
        let fleet: String = row.try_get(0).map_err(query(CONTEXT_TARGET))?;
        let event_id: String = row.try_get(1).map_err(query(CONTEXT_TARGET))?;
        Ok(Some(Target {
            fleet_id: Uuid7::parse(&fleet)
                .map_err(row_malformed("fleet.runner_leases", "fleet_id"))?,
            event_id,
        }))
    }
}

impl<'a> Published<'a> {
    /// The dashboard's shape for one wire frame.
    ///
    /// Total over [`ActivityFrame`], so a frame variant added upstream fails to
    /// compile here until its channel name is decided — which is the property
    /// the Zig gets from its exhaustive `switch` and the reason this is a match
    /// rather than a serde re-tag.
    ///
    /// # Errors
    /// Reports arguments that are not well-formed JSON. Only the started frame
    /// can fail, because it is the only one carrying a nested document.
    fn of(
        target: &'a Target,
        frame: &'a ActivityFrame<'a>,
    ) -> core::result::Result<Self, serde_json::Error> {
        let event_id = target.event_id.as_str();
        Ok(match frame {
            ActivityFrame::ToolCallStarted(body) => Self::ToolCallStarted {
                event_id,
                name: &body.name,
                args_redacted: serde_json::from_str(&body.args_redacted)?,
            },
            ActivityFrame::ToolCallProgress(body) => Self::ToolCallProgress {
                event_id,
                name: &body.name,
                elapsed_ms: body.elapsed_ms,
            },
            ActivityFrame::FleetResponseChunk(body) => Self::Chunk {
                event_id,
                text: &body.text,
            },
            ActivityFrame::ToolCallCompleted(body) => Self::ToolCallCompleted {
                event_id,
                name: &body.name,
                ms: body.ms,
            },
        })
    }
}

impl crate::lease::pull::Plane {
    /// Forward one batch of live-tail frames for a lease this runner holds.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, and reports a datastore that
    /// would not answer. A queue that will not take a frame is NOT an error —
    /// see [`Leases::publish_activity`].
    pub async fn activity(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        frames: &[ActivityFrame<'_>],
    ) -> Result<()> {
        let Some(target) = self
            .leases
            .load_activity_target(lease_id, runner_id)
            .await?
        else {
            return Err(crate::error::lease_not_found());
        };
        self.leases.publish_activity(&target, frames).await;
        Ok(())
    }
}
