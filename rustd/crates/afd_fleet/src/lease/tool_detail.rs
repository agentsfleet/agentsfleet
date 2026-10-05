//! The tool-call record verb: each finished call's full arguments and output,
//! kept under the lease's fence for "show all".
//!
//! Shaped like the memory push (`lease/memory.rs`): the runner names its lease,
//! the statement proves the runner holds it live, and a holder a reclaim has
//! superseded writes nothing. Best-effort for the run — a record that breaks a
//! bound is skipped and counted, and the run's answer never depends on a post.
//!
//! # The budget is per event and per fence
//!
//! Settlement keeps one fence's records (`DELETE_OTHER_FENCES`), so the set an
//! operator can open is one lease's. A reclaimed lease therefore starts with
//! the whole budget rather than inheriting a dead lease's spend.

use std::collections::BTreeMap;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::tool_detail::{
    DETAIL_EVENT_MAX_BYTES, DetailRejection, RawToolCallRecord, ToolCallRecord,
    ToolCallRecordsRequest, ToolCallRecordsStored,
};

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::pull::Plane;

/// A record was not kept; the run is unaffected.
const EVENT_SKIPPED: &str = "tool_detail_skipped";

/// A superseded holder posted records; nothing was kept.
const EVENT_FENCED: &str = "tool_detail_fenced";

/// The lease a post names, as the statement proved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetailTarget {
    fleet_id: String,
    workspace_id: String,
    event_id: String,
    /// The lease's own fencing token, which keys every row it writes.
    fence: i64,
    /// The fleet's live sequence.
    live_seq: i64,
}

impl DetailTarget {
    /// Whether this lease still holds the fleet, and the post is its own.
    fn holds(&self, presented: u64) -> bool {
        crate::lease::standing::holds(self.fence, self.live_seq, presented)
    }
}

/// A record that passed its own bounds, and where it sat in the post.
#[derive(Debug)]
struct Admissible<'a> {
    record: ToolCallRecord<'a>,
    position: usize,
    bytes: usize,
}

/// One record a post did not keep, for the line it is logged under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Skip {
    position: usize,
    reason: DetailRejection,
    bytes: usize,
}

/// Each posted record narrowed on its own, keyed by call number.
///
/// A call named twice in one post keeps its last record; the earlier one is
/// skipped as malformed, because a post naming a call twice is not one the
/// runner meant to send.
fn narrow_all<'a>(calls: &[RawToolCallRecord<'a>]) -> (BTreeMap<u64, Admissible<'a>>, Vec<Skip>) {
    let mut kept = BTreeMap::new();
    let mut skipped = Vec::new();
    for (position, raw) in calls.iter().enumerate() {
        match raw.narrow() {
            Ok(record) => {
                let bytes = record.byte_count();
                let admissible = Admissible {
                    record,
                    position,
                    bytes,
                };
                if let Some(earlier) = kept.insert(admissible.record.call_number, admissible) {
                    skipped.push(Skip {
                        position: earlier.position,
                        reason: DetailRejection::Malformed,
                        bytes: earlier.bytes,
                    });
                }
            }
            Err(reason) => skipped.push(Skip {
                position,
                reason,
                bytes: raw.byte_len(),
            }),
        }
    }
    (kept, skipped)
}

/// The records that fit what the event has left, lowest call number first.
///
/// `spent` is everything the lease already keeps, and `replaced` what each
/// posted call's current record spends: keeping a replacement swaps its old
/// size for its new one, and skipping it leaves the old record — and its
/// spend — in place.
fn within_budget<'a>(
    candidates: BTreeMap<u64, Admissible<'a>>,
    spent: usize,
    replaced: &BTreeMap<u64, usize>,
) -> (Vec<Admissible<'a>>, Vec<Skip>) {
    let mut total = spent;
    let mut kept = Vec::new();
    let mut over = Vec::new();
    for (number, admissible) in candidates {
        let old = replaced.get(&number).copied().unwrap_or_default();
        let after = total.saturating_sub(old).saturating_add(admissible.bytes);
        if after > DETAIL_EVENT_MAX_BYTES {
            over.push(Skip {
                position: admissible.position,
                reason: DetailRejection::OverBudget,
                bytes: admissible.bytes,
            });
        } else {
            total = after;
            kept.push(admissible);
        }
    }
    (kept, over)
}

/// What one post's transaction decided.
#[derive(Debug)]
enum Kept {
    /// No live lease of this runner's under that id.
    NoLease,
    /// The lease is not the fleet's current holder, or the post is not its own.
    Fenced(DetailTarget),
    /// Written: how many records, and those that did not fit.
    Stored {
        target: DetailTarget,
        stored: usize,
        over: Vec<Skip>,
    },
}

impl Plane {
    /// Keep the full record of each finished call a lease posts.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not live, and a holder the
    /// fleet has superseded; neither writes anything. Reports a datastore that
    /// would not answer. A record refused for its shape, a bound or the
    /// event's budget is counted, not an error.
    pub async fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: UnixMillis,
    ) -> Result<ToolCallRecordsStored> {
        let (candidates, mut skipped) = narrow_all(&request.calls);
        let presented = request.fencing_token;
        let decided = self
            .leases
            .keep_records(lease_id, runner_id, presented, candidates, now)
            .await?;
        let (target, stored_count, over) = match decided {
            Kept::NoLease => return Err(lease_not_found()),
            Kept::Fenced(target) => {
                let fleet_id = target.fleet_id.as_str();
                let live_seq = target.live_seq;
                tracing::debug!(
                    fleet_id,
                    fencing_token = presented,
                    live_seq,
                    event = EVENT_FENCED
                );
                return Err(stale_fence());
            }
            Kept::Stored {
                target,
                stored,
                over,
            } => (target, stored, over),
        };
        skipped.extend(over);
        for skip in &skipped {
            log_skip(&target, *skip);
        }
        Ok(ToolCallRecordsStored {
            stored_count,
            skipped_count: skipped.len(),
        })
    }
}

/// One skipped record, by position and size; its content is never logged.
fn log_skip(target: &DetailTarget, skip: Skip) {
    let fleet_id = target.fleet_id.as_str();
    let event_id = target.event_id.as_str();
    let Skip {
        position,
        reason,
        bytes,
    } = skip;
    let reason = reason.as_str();
    tracing::info!(
        fleet_id,
        agentsfleet_event_id = event_id,
        position,
        reason,
        bytes,
        event = EVENT_SKIPPED
    );
}

#[path = "tool_detail/store.rs"]
mod store;

#[cfg(test)]
#[path = "tool_detail/tests.rs"]
mod tests;
