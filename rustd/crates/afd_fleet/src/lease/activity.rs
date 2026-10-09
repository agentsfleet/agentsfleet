//! The activity verb: live-tail frames a runner forwards while its child works.
//!
//! A runner holds no Dragonfly, so it ships progress frames here and the daemon
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
//! below has no `status` predicate either. The fence is read all the same, to
//! scope the call ids a lease publishes (`activity/published.rs`).
//!
//! # The vocabulary bridge
//!
//! This is the one seam where the runner's frame names become the dashboard's.
//! They are NOT the same vocabulary: `fleet_response_chunk` on the wire is
//! `chunk` on the channel, and that single rename is the whole reason a
//! translation type exists rather than the wire frame being re-serialized.
//! `Published::of` is total over [`ActivityFrame`], so a new frame variant
//! fails the build until somebody decides what the dashboard calls it.

use afd_core::id::Uuid7;
use afd_observability::metrics::label::fleet::DeliveryStage;
use afd_observability::producers;
use afd_wire::activity::ActivityFrame;
use sqlx::Row as _;

use self::published::Published;
use crate::error::{Result, query, row_malformed};
use crate::lease::sql;
use crate::lease::store::Leases;

mod published;

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
    /// Both instants are stamped by the control plane, so their differences
    /// from activity receipt do not compare clocks across hosts.
    pub lease_created_at: i64,
    /// Producer event timestamp in epoch milliseconds.
    pub event_created_at: i64,
    /// Expired or superseded holders can publish cosmetic frames but cannot
    /// contribute a latency sample to the active-lease histogram.
    pub timing_eligible: bool,
    /// The lease's fencing token, distinct per claim of the fleet. Kept as the
    /// column's own `i64`: it only scopes a cosmetic id, so a value this daemon
    /// never writes is no reason to refuse the tail.
    pub fence: i64,
}

impl Leases {
    /// Publish one batch of frames to `target`'s channel.
    ///
    /// Never fails the verb. Each frame is encoded independently so one
    /// unencodable frame does not silence the rest of the batch. The encodable
    /// frames are published together in order; a Dragonfly outage costs the
    /// tail rather than the run.
    pub async fn publish_activity(&self, target: &Target, frames: &[ActivityFrame<'_>]) {
        let fleet = target.fleet_id.as_str();
        let streams = self.streams();
        let mut payloads = Vec::with_capacity(frames.len());
        for frame in frames {
            // `args_redacted` arrives as a STRING holding JSON. Parsing it into
            // a tree only to splice it back out would build and drop a whole
            // value per tool call. `RawValue` says "these bytes are already
            // JSON": it validates the syntax and keeps the bytes, so the frame
            // is checked without ever being materialised.
            let owned;
            let published = match Published::of(target, frame) {
                Ok(value) => {
                    owned = value;
                    &owned
                }
                Err(malformed) => {
                    tracing::debug!(
                        fleet_id = fleet,
                        reason = %malformed,
                        event = EVENT_DROPPED,
                        "a frame carried arguments that are not JSON; the tail loses it"
                    );
                    continue;
                }
            };
            // Strings, integers, flags and JSON `Published::of` already
            // validated: serializing that to a `String` cannot fail, so there is
            // no drop arm to take.
            payloads.extend(serde_json::to_string(published).ok());
        }
        if let Err(unreachable_queue) = streams.publish_tail_batch(fleet, &payloads).await {
            let reason = unreachable_queue.to_string();
            tracing::debug!(
                fleet_id = fleet,
                reason,
                event = EVENT_DROPPED,
                "the queue would not take a live-tail batch; the run is unaffected"
            );
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
        let lease_created_at: i64 = row.try_get(2).map_err(query(CONTEXT_TARGET))?;
        let event_created_at: i64 = row.try_get(3).map_err(query(CONTEXT_TARGET))?;
        let status: String = row.try_get(4).map_err(query(CONTEXT_TARGET))?;
        let lease_expires_at: i64 = row.try_get(5).map_err(query(CONTEXT_TARGET))?;
        let fence: i64 = row.try_get(6).map_err(query(CONTEXT_TARGET))?;
        Ok(Some(Target {
            fleet_id: Uuid7::parse(&fleet)
                .map_err(row_malformed("fleet.runner_leases", "fleet_id"))?,
            event_id,
            lease_created_at,
            event_created_at,
            timing_eligible: status == sql::LEASE_STATUS_ACTIVE
                && lease_expires_at > afd_core::clock::now().as_millis(),
            fence,
        }))
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
        record_first_chunk(&target, frames);
        self.leases.publish_activity(&target, frames).await;
        Ok(())
    }
}

/// The first chunk carries the runner's agent-relative duration once; all later
/// chunks carry none. The two daemon-side samples come from one clock read and
/// are skipped if the clock moved back.
fn record_first_chunk(target: &Target, frames: &[ActivityFrame<'_>]) {
    if !target.timing_eligible {
        return;
    }
    let Some(first_ms) = first_visible_candidate_ms(frames) else {
        return;
    };
    let received_at = afd_core::clock::now().as_millis();
    for (stage, since) in [
        (DeliveryStage::LeaseToFirstChunk, target.lease_created_at),
        (DeliveryStage::EventToFirstChunk, target.event_created_at),
    ] {
        if let Ok(millis) = u64::try_from(received_at.saturating_sub(since)) {
            producers::fleet::delivery_stage(stage, core::time::Duration::from_millis(millis));
        }
    }
    producers::fleet::delivery_stage(
        DeliveryStage::ZombieToFirstChunk,
        core::time::Duration::from_millis(first_ms),
    );
}

fn first_visible_candidate_ms(frames: &[ActivityFrame<'_>]) -> Option<u64> {
    frames.iter().find_map(|frame| match frame {
        ActivityFrame::FleetResponseChunk(body)
            if body.text_kind.is_some()
                && body.stream_start
                && body.stream_contiguous
                && body.stream_seq == 0 =>
        {
            body.first_chunk_after_ms
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests;
