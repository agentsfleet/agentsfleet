//! The tool-call record verb: each finished call's full arguments and output,
//! kept under the run's fence for "show all".
//!
//! Shaped like the memory push (`run/memory.rs`): the runner names its run,
//! the statement proves the runner holds it live, and a holder a reassignment has
//! superseded writes nothing. Best-effort for the run — a record that breaks a
//! bound is skipped and counted, and the run's answer never depends on a post.
//!
//! # No size caps a record
//!
//! A record keeps the call's whole output and arguments: no field cap, no
//! budget per run, and no post cap beyond the runner frame the route reads a
//! body up to. Settlement keeps one run's records (the report statement drops
//! the rest, as `DELETE_OTHER_FENCES` does for a post), so the set an operator
//! can open is one run's.
//!
//! # Each record names a turn the run serves
//!
//! A record names the turn newest when its call's step began: the run's own,
//! or one a person sent that its conversation folded in. A record naming any
//! other turn is skipped as `unserved_turn` and counted, and the statement
//! that writes the rest joins only a served turn, so none is ever stored.

use std::collections::{BTreeMap, BTreeSet};

use af_core::id::Uuid7;
use af_wire::tool_detail::{
    DetailRejection, RawToolCallRecord, ToolCallRecord, ToolCallRecordsRequest,
    ToolCallRecordsStored,
};
use jiff::Timestamp;

use crate::error::{Result, run_not_found, stale_fence};
use crate::run::pull::RunService;
use crate::run::standing::LiveRun;

/// A record was not kept; the run is unaffected.
const EVENT_SKIPPED: &str = "tool_detail_skipped";

/// A superseded holder posted records; nothing was kept.
const EVENT_FENCED: &str = "tool_detail_fenced";

/// A record that passed its own bounds, where it sat in the post, and how
/// many bytes the runner sent for it.
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
                let admissible = Admissible {
                    record,
                    position,
                    bytes: raw.byte_len(),
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

/// The candidates whose turn is one of `served`, and a skip for each that
/// names another.
fn serving<'a>(
    candidates: BTreeMap<u64, Admissible<'a>>,
    served: &BTreeSet<String>,
) -> (BTreeMap<u64, Admissible<'a>>, Vec<Skip>) {
    let mut kept = BTreeMap::new();
    let mut unserved = Vec::new();
    for (number, admissible) in candidates {
        if served.contains(admissible.record.turn_id.as_ref()) {
            kept.insert(number, admissible);
        } else {
            unserved.push(Skip {
                position: admissible.position,
                reason: DetailRejection::UnservedTurn,
                bytes: admissible.bytes,
            });
        }
    }
    (kept, unserved)
}

/// Every turn `candidates` name, once each. The record's own bound proved
/// each a canonical version 7 UUID, so none is lost to the parse.
fn named_turns(candidates: &BTreeMap<u64, Admissible<'_>>) -> Vec<Uuid7> {
    let named: BTreeSet<&str> = candidates
        .values()
        .map(|admissible| admissible.record.turn_id.as_ref())
        .collect();
    named
        .into_iter()
        .filter_map(|turn| Uuid7::parse(turn).ok())
        .collect()
}

/// What one post's transaction decided.
#[derive(Debug)]
enum Kept {
    /// No live run of this runner's under that id.
    NoRun,
    /// The run is not the fleet's current holder, or the post is not its own.
    Fenced(LiveRun),
    /// Written: how many records, and those refused for naming a turn the
    /// run does not serve.
    Stored {
        target: LiveRun,
        stored: usize,
        unserved: Vec<Skip>,
    },
}

impl RunService {
    /// Keep the full record of each finished call a run posts.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not live, and a holder the
    /// fleet has superseded; neither writes anything. Reports a datastore that
    /// would not answer. A record refused for its shape or for a turn the run
    /// does not serve is counted, not an error.
    pub async fn record_tool_calls(
        &self,
        runner_id: &Uuid7,
        run_id: &str,
        request: &ToolCallRecordsRequest<'_>,
        now: Timestamp,
    ) -> Result<ToolCallRecordsStored> {
        let run_id = crate::error::wire_run(run_id)?;
        let (candidates, mut skipped) = narrow_all(&request.calls);
        let presented = request.fencing_token;
        let decided = self
            .runs
            .keep_records(&run_id, runner_id, presented, candidates, now)
            .await?;
        let (target, stored_count, unserved) = match decided {
            Kept::NoRun => return Err(run_not_found()),
            Kept::Fenced(target) => {
                let fleet_id = target.fleet_id.as_str();
                let live_seq = target.live_seq;
                {
                    let (fencing_token, event) = (&presented, &EVENT_FENCED);
                    tracing::debug!(fleet_id, fencing_token, live_seq, event);
                };
                return Err(stale_fence());
            }
            Kept::Stored {
                target,
                stored,
                unserved,
            } => (target, stored, unserved),
        };
        skipped.extend(unserved);
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
fn log_skip(target: &LiveRun, skip: Skip) {
    let fleet_id = target.fleet_id.as_str();
    let turn_id = target.turn_id.as_str();
    let Skip {
        position,
        reason,
        bytes,
    } = skip;
    let reason = reason.as_str();
    {
        let (agentsfleet_turn_id, event) = (&turn_id, &EVENT_SKIPPED);
        tracing::info!(
            fleet_id,
            agentsfleet_turn_id,
            position,
            reason,
            bytes,
            event
        );
    };
}

mod store;

#[cfg(test)]
mod tests;
