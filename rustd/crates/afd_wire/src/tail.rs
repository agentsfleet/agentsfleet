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
    },
    /// A human has been asked about one of the fleet's actions.
    GateOpened {
        /// The gate's row identifier.
        gate_id: Cow<'a, str>,
        /// The event the gate holds.
        event_id: Cow<'a, str>,
        /// How many approvals wait on the fleet, this one included.
        pending_approvals: i64,
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
    },
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use std::borrow::Cow;

    use serde_json::{Value, json};

    use super::{TailFrame, TailRow};
    use crate::event::EventSummary;

    /// The two columns the channel names and the completion leaves off.
    const SCOPE_KEYS: [&str; 2] = ["fleet_id", "workspace_id"];

    /// The anchor the SSE layer reads the frame's name from.
    const KIND_ANCHOR: &str = "{\"kind\":\"";

    fn row() -> EventSummary<'static> {
        EventSummary {
            fleet_id: Cow::Borrowed("fleet-1"),
            event_id: Cow::Borrowed("1725000000000-0"),
            workspace_id: Cow::Borrowed("ws-1"),
            actor: Cow::Borrowed("steer:user_1"),
            event_type: Cow::Borrowed("chat"),
            status: Cow::Borrowed("processed"),
            tokens: Some(1200),
            wall_ms: Some(12_000),
            failure_label: None,
            failure_detail: None,
            checkpoint_id: None,
            resumes_event_id: None,
            created_at: 1_725_000_000_000,
            updated_at: 1_725_000_012_000,
            cost_nanos: Some(40_000_000),
        }
    }

    fn rendered(frame: &TailFrame<'_>) -> (String, Value) {
        let text = serde_json::to_string(frame).expect("every tail frame serializes");
        let value = serde_json::from_str(&text).expect("and is JSON");
        (text, value)
    }

    /// The name is the leading field on every variant, which is the property
    /// `afd_sse` dispatches on and the one a field reorder would silently lose.
    #[test]
    fn should_lead_every_frame_with_its_kind() {
        let frames = [
            TailFrame::EventReceived {
                event_id: Cow::Borrowed("e"),
                actor: Cow::Borrowed("cron"),
                event_type: Cow::Borrowed("cron"),
                created_at: 1,
            },
            TailFrame::EventComplete {
                event: Box::new(TailRow::from(row())),
                fleet_status: Cow::Borrowed("active"),
                pending_approvals: 0,
            },
            TailFrame::GateOpened {
                gate_id: Cow::Borrowed("g"),
                event_id: Cow::Borrowed("e"),
                pending_approvals: 1,
            },
            TailFrame::GateResolved {
                gate_id: Cow::Borrowed("g"),
                event_id: Some(Cow::Borrowed("e")),
                status: Cow::Borrowed("approved"),
                resolved_by: Cow::Borrowed("human:x"),
                pending_approvals: 0,
            },
        ];
        let expected = [
            "event_received",
            "event_complete",
            "gate_opened",
            "gate_resolved",
        ];
        for (frame, kind) in frames.iter().zip(expected) {
            let (text, value) = rendered(frame);
            assert!(
                text.starts_with(&format!("{KIND_ANCHOR}{kind}\"")),
                "{kind} must lead its payload: {text}"
            );
            assert_eq!(value["kind"], json!(kind));
        }
    }

    /// The completion is the events-list row plus the two fleet facts, and
    /// nothing the row carries is renamed or dropped on the way but the two
    /// scope columns — the client folds it with the same decoder it uses for
    /// a backfilled page, and the workspace multiplex adds the scope back as
    /// its one leading `fleet_id`.
    #[test]
    fn should_carry_the_whole_terminal_row_on_a_completion() {
        let (text, value) = rendered(&TailFrame::EventComplete {
            event: Box::new(TailRow::from(row())),
            fleet_status: Cow::Borrowed("paused"),
            pending_approvals: 2,
        });
        let row_value = serde_json::to_value(row()).expect("the row serializes");
        for (key, expected) in row_value.as_object().expect("a row is an object") {
            if SCOPE_KEYS.contains(&key.as_str()) {
                assert!(
                    value.get(key).is_none(),
                    "{key} is the channel's to name, not the frame's: {text}"
                );
                continue;
            }
            assert_eq!(&value[key], expected, "{key} rides the frame unchanged");
        }
        assert_eq!(value["fleet_status"], json!("paused"));
        assert_eq!(value["pending_approvals"], json!(2));
        assert_eq!(value["cost_nanos"], json!(40_000_000));
    }

    /// A gate that held no run says so with `null`, the crate's spelling for
    /// an absent optional — never an empty string a consumer keying on the
    /// identifier would take for a real one.
    #[test]
    fn should_spell_a_runless_gates_event_as_null() {
        let (text, value) = rendered(&TailFrame::GateResolved {
            gate_id: Cow::Borrowed("g"),
            event_id: None,
            status: Cow::Borrowed("timed_out"),
            resolved_by: Cow::Borrowed("sweeper"),
            pending_approvals: 0,
        });
        assert_eq!(value["event_id"], Value::Null, "{text}");
    }
}
