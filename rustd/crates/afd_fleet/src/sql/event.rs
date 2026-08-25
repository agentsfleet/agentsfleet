//! `core.fleet_events` — the narrative log a lease opens and a report closes.
//!
//! A different table family from `sql::lease`, and a different lifetime: the
//! lease row records who OWNS work, this one records that the work exists and
//! how it ended. The reclaim path joins them, which is the only place the two
//! meet.
//!
//! Text is byte-identical to `fleet/sql.zig`.

/// Record an inbound event.
///
/// `ON CONFLICT DO NOTHING` on `(fleet_id, event_id)` is the idempotence
/// boundary for redelivery: the same event arriving twice writes one row, so a
/// retrying producer cannot double-run a fleet. It is also what makes the
/// reclaim path safe — a re-leased event finds its row already there.
///
/// `$1` fleet, `$2` event, `$3` workspace, `$4` actor, `$5` type, `$6` body,
/// `$7` resumes-event, `$8` now, `$9` status.
pub const INSERT_FLEET_EVENT: &str = "\
INSERT INTO core.fleet_events
  (fleet_id, event_id, workspace_id, actor, event_type,
   status, request_json, resumes_event_id, created_at, updated_at)
VALUES ($1::uuid, $2, $3::uuid, $4, $5, $9, $6::jsonb, $7, $8, $8)
ON CONFLICT (fleet_id, event_id) DO NOTHING";

/// The status an event opens in.
///
/// `event_rows.zig`'s `received`. The report flips it to `processed` or
/// `fleet_error`, and the gate pass to `gate_blocked` — all three read this
/// spelling as their starting point, so it is declared once (RULE UFS).
pub const EVENT_STATUS_RECEIVED: &str = "received";
