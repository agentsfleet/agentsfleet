//! The frames the daemon itself publishes on a fleet's live tail.
//!
//! The runner forwards its mid-run frames through [`crate::activity`]; these
//! are the daemon's own. The brackets open and close a run — `event_received`
//! when the lease verb records the row, `event_complete` when a report or a
//! gate refusal ends it — and the gate frames say when a human was asked and
//! when they answered. All of them ride `fleet:{id}:activity` beside the
//! runner's, and the dashboard's stream registry and the CLI's steer tail
//! switch on the `kind` spellings here (`ui/packages/app/lib/api/events.ts`,
//! `cli/src/commands/fleet_steer_events.ts`).
//!
//! # Every frame is self-sufficient
//!
//! A watcher folds a frame into what it shows and issues no read to complete
//! it. So the completion carries the terminal row exactly as the events list
//! would serve it, plus the two facts about the fleet a run can change: its
//! lifecycle status, and how many approvals wait on it. The gate frames carry
//! the count for the same reason — the moment a gate opens is the moment an
//! operator watching the chat needs the "approvals waiting" link to appear.
//!
//! # `kind` leads, and that is the SSE layer's contract
//!
//! `afd_sse::frame::kind_of` names the `event:` line from the payload's
//! LEADING field. The tag attribute writes it first; nothing else here may.
//! Serialize-only on purpose: the daemon authors these and nothing parses
//! them back in this workspace, so a `Deserialize` would be a fixture nobody
//! round-trips.
//!
//! # The scope is the channel's, never the frame's
//!
//! No frame here names its fleet or workspace. The tail is one fleet's
//! channel, so the scope is known to whoever subscribed; and the workspace
//! multiplex splices `fleet_id` in as the leading key of every frame it
//! forwards (`afd_sse::Frame::tagged`). A completion that carried the row's
//! own `fleet_id` would put the key on the wire twice on that stream — the
//! same value, but a duplicate key is one a strict decoder refuses — so the
//! completion carries [`TailRow`], the events-list row with its two scope
//! columns left off.

use std::borrow::Cow;

use serde::Serialize;

use crate::event::EventSummary;

/// The terminal row as a completion carries it: [`EventSummary`] without the
/// two scope columns the channel already names.
///
/// Field for field the listing's shape and order past those two, so the
/// client folds a completion with the decoder it uses for a backfilled page.
/// Built only from a summary, so the one row-to-wire mapping stays the
/// summary's; the test below pins that nothing but the scope is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TailRow<'a> {
    /// The canonical event identifier.
    pub event_id: Cow<'a, str>,
    /// Who or what produced the event.
    pub actor: Cow<'a, str>,
    /// How the event entered the system, as stored.
    pub event_type: Cow<'a, str>,
    /// Where the event's run got to, as stored.
    pub status: Cow<'a, str>,
    /// Tokens the run spent, absent until a runner reports.
    pub tokens: Option<i64>,
    /// Wall milliseconds the run took, absent until a runner reports.
    pub wall_ms: Option<i64>,
    /// What refused or failed the run, absent on a clean one.
    pub failure_label: Option<Cow<'a, str>>,
    /// The operator-readable cause line, absent when none was carried.
    pub failure_detail: Option<Cow<'a, str>>,
    /// The session checkpoint this run wrote, when it wrote one.
    pub checkpoint_id: Option<Cow<'a, str>>,
    /// The event this one continues, set on a continuation.
    pub resumes_event_id: Option<Cow<'a, str>>,
    /// Epoch milliseconds the row was created.
    pub created_at: i64,
    /// Epoch milliseconds the row last changed.
    pub updated_at: i64,
    /// What this event actually cost, summed over its telemetry rows.
    pub cost_nanos: Option<i64>,
}

impl<'a> From<EventSummary<'a>> for TailRow<'a> {
    fn from(row: EventSummary<'a>) -> Self {
        let EventSummary {
            event_id,
            actor,
            event_type,
            status,
            tokens,
            wall_ms,
            failure_label,
            failure_detail,
            checkpoint_id,
            resumes_event_id,
            created_at,
            updated_at,
            cost_nanos,
            ..
        } = row;
        Self {
            event_id,
            actor,
            event_type,
            status,
            tokens,
            wall_ms,
            failure_label,
            failure_detail,
            checkpoint_id,
            resumes_event_id,
            created_at,
            updated_at,
            cost_nanos,
        }
    }
}

/// Where a fleet's counters stand, as the database has them.
///
/// Absolute, and that is the whole design. A frame carrying a DIFFERENCE has to
/// arrive exactly once to leave a client correct; a frame carrying the total
/// leaves it correct however many times it arrives, or does not. A redelivery
/// changes nothing, a dropped frame is corrected by the next one, and no
/// publisher has to know which increment is its own.
///
/// Both counters ride every frame rather than each riding the frame that moved
/// it, because they do not move together. `events_processed` is bumped
/// `AFTER INSERT ON core.fleet_events` (`schema/890`) — at RECEIVE, not at
/// completion — while `budget_used_nanos` follows the ledger write, which may
/// update one row many times across a run. Both only ever grow — the
/// triggers add, and the backfill's conflict arm takes `GREATEST` — so a
/// client that keeps the greater of what it holds and what a frame carries is
/// right under either clock, and right when frames cross in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FleetCounters {
    /// Lifetime event count. Server truth, never client arithmetic.
    pub events_processed: i64,
    /// Lifetime spend, in nanos.
    pub budget_used_nanos: i64,
}

/// One frame the daemon publishes on a fleet's tail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TailFrame<'a> {
    /// The narrative log opened: a run is about to start.
    EventReceived {
        /// The canonical event identifier.
        event_id: Cow<'a, str>,
        /// Who or what produced the event.
        actor: Cow<'a, str>,
        /// How the event entered the system.
        event_type: Cow<'a, str>,
        /// Epoch milliseconds the row was created.
        created_at: i64,
        /// Where the fleet's counters stand after this frame — absolute, so
        /// the client ASSIGNS them rather than adding to them.
        ///
        /// `None` when the read that would have filled them did not answer.
        /// A frame is best-effort (see the module docs) and a counter the
        /// daemon could not read is better absent than guessed: the client
        /// leaves the figures it already has standing.
        #[serde(flatten)]
        counters: Option<FleetCounters>,
    },
    /// The narrative log closed: the run ended, and here is the row.
    EventComplete {
        /// The terminal row, field for field as the events list serves it,
        /// less the scope the channel names.
        ///
        /// Boxed so the enum stays the size of its other arms: a completion
        /// is built once per run, and the other three frames are what the
        /// gate paths build on their own hot paths.
        #[serde(flatten)]
        event: Box<TailRow<'a>>,
        /// The fleet's lifecycle status after the run.
        fleet_status: Cow<'a, str>,
        /// How many approvals wait on the fleet after the run.
        pending_approvals: i64,
        /// Where the fleet's counters stand after this frame — absolute, so
        /// the client ASSIGNS them rather than adding to them.
        ///
        /// `None` when the read that would have filled them did not answer.
        /// A frame is best-effort (see the module docs) and a counter the
        /// daemon could not read is better absent than guessed: the client
        /// leaves the figures it already has standing.
        #[serde(flatten)]
        counters: Option<FleetCounters>,
    },
    /// A human has been asked about one of the fleet's actions.
    GateOpened {
        /// The gate's row identifier.
        gate_id: Cow<'a, str>,
        /// The event the gate holds.
        event_id: Cow<'a, str>,
        /// How many approvals wait on the fleet, this one included.
        pending_approvals: i64,
        /// Where the fleet's counters stand after this frame — absolute, so
        /// the client ASSIGNS them rather than adding to them.
        ///
        /// `None` when the read that would have filled them did not answer.
        /// A frame is best-effort (see the module docs) and a counter the
        /// daemon could not read is better absent than guessed: the client
        /// leaves the figures it already has standing.
        #[serde(flatten)]
        counters: Option<FleetCounters>,
    },
    /// A human answered, or the window closed with no answer.
    GateResolved {
        /// The gate's row identifier.
        gate_id: Cow<'a, str>,
        /// The event the gate held, or `null` for a gate that held no run —
        /// a standing grant raised at install time parks no event.
        event_id: Option<Cow<'a, str>>,
        /// Where the gate now stands: approved, denied, timed out.
        status: Cow<'a, str>,
        /// Who answered — a person, or the sweeper.
        resolved_by: Cow<'a, str>,
        /// How many approvals still wait on the fleet.
        pending_approvals: i64,
        /// Where the fleet's counters stand after this frame — absolute, so
        /// the client ASSIGNS them rather than adding to them.
        ///
        /// `None` when the read that would have filled them did not answer.
        /// A frame is best-effort (see the module docs) and a counter the
        /// daemon could not read is better absent than guessed: the client
        /// leaves the figures it already has standing.
        #[serde(flatten)]
        counters: Option<FleetCounters>,
    },
}

#[cfg(test)]
mod tests;
