//! `core.fleet_events` — every statement that touches the table, in one place.
//!
//! Text is byte-identical to `fleet/sql.zig`.
//!
//! # Why the statements are public and the reads are not
//!
//! A write here binds a caller's domain type — an [`Acquired`] lease, a gate's
//! `Refusal`, a runner's verdict — and those types belong to the runner plane.
//! Moving the binding down here would drag them with it and invert the
//! layering, so the writers keep their `bind` chains and this module owns the
//! one text each of them runs. That is the whole of what was wrong before:
//! `afd_approval` carried a byte-identical copy of [`INSERT_FLEET_EVENT`]
//! because reaching the original meant depending on 25,500 lines of runner
//! plane to reuse ten lines of SQL.
//!
//! The READS bind nothing from anywhere else — a filter, a cursor, a limit —
//! so they are verbs on [`History`](crate::History) rather than exported text.
//!
//! [`Acquired`]: https://docs.rs/afd_fleet

/// Record an inbound event.
///
/// `ON CONFLICT DO NOTHING` on `(fleet_id, event_id)` is the idempotence
/// boundary for redelivery: the same event arriving twice writes one row, so a
/// retrying producer cannot double-run a fleet. It is also what makes the
/// reclaim path safe — a re-leased event finds its row already there — and what
/// makes a retried approval resolve continue its run exactly once.
///
/// `$1` fleet, `$2` event, `$3` workspace, `$4` actor, `$5` type, `$6` body,
/// `$7` resumes-event, `$8` now, `$9` status.
pub const INSERT_FLEET_EVENT: &str = "\
INSERT INTO core.fleet_events
  (fleet_id, event_id, workspace_id, actor, event_type,
   status, request_json, resumes_event_id, created_at, updated_at)
VALUES ($1::uuid, $2, $3::uuid, $4, $5, $9, $6::jsonb, $7, $8, $8)
ON CONFLICT (fleet_id, event_id) DO NOTHING";

/// End an event at a gate, naming what refused it.
///
/// Guarded on `status = $6` — always [`status::RECEIVED`] — so a terminal row
/// is NEVER reopened. A request arriving after a `gate_blocked` is a new
/// delivery with its own row (RULE IDMP), not a resurrection of this one, and
/// the guard is what makes that structural rather than conventional.
///
/// Zero rows affected is not an error: it means the row was already terminal,
/// which happens when a refused delivery's earlier acknowledgement was lost.
/// The acknowledgement is still owed, so the caller proceeds — which is why
/// this returns a count rather than a success flag.
///
/// `NULLIF($7, '')` keeps the established row shape for the callers that carry
/// no operator-readable detail: an empty detail stores `NULL`, not `''`, so a
/// consumer testing `IS NULL` cannot be fooled by an empty string.
///
/// `$1` fleet, `$2` event, `$3` new status, `$4` failure label, `$5` now,
/// `$6` the status this transition is guarded on, `$7` detail.
pub const UPDATE_FLEET_EVENT_FAILURE: &str = "\
UPDATE core.fleet_events
SET status = $3, failure_label = $4, updated_at = $5,
    failure_detail = NULLIF($7, '')
WHERE fleet_id = $1::uuid AND event_id = $2 AND status = $6";

/// End an event with the runner's verdict.
///
/// The sibling of [`UPDATE_FLEET_EVENT_FAILURE`] and guarded the same way, on
/// `status = $9` — always [`status::RECEIVED`]. A terminal row is never
/// reopened, so a redelivery whose acknowledgement was lost moves zero rows and
/// the settled result stands. That is the whole reason the guard is in the
/// statement rather than in a prior read: a check-then-write would let two
/// reports of one event race, and the second would overwrite the first.
///
/// `failure_label` and `failure_detail` are NULL on a clean run, structurally
/// rather than by convention — the caller's verdict type has nowhere to carry a
/// cause on the succeeded arm.
///
/// `$1` fleet, `$2` event, `$3` new status, `$4` response text, `$5` tokens,
/// `$6` wall milliseconds, `$7` now, `$8` failure label, `$9` the status this
/// transition is guarded on, `$10` failure detail.
pub const UPDATE_FLEET_EVENT_RESULT: &str = "\
UPDATE core.fleet_events
SET status = $3, response_text = $4, tokens = $5, wall_ms = $6, updated_at = $7, failure_label = $8, failure_detail = $10
WHERE fleet_id = $1::uuid AND event_id = $2 AND status = $9";

/// One event's current status.
///
/// Read on ONE path only: a redelivery whose insert hit the conflict arm. The
/// question it answers is whether that redelivery is a legitimate re-poll (the
/// row is still `received` — a parked gate, a reclaimed strand) or an already
/// settled event whose acknowledgement was lost. Re-running the second would
/// double-fire side effects and re-meter tokens, so the two must not be guessed
/// between.
///
/// `$1` fleet, `$2` event.
pub const SELECT_FLEET_EVENT_STATUS: &str = "\
SELECT status FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2";

/// The byte ceiling on a stored failure cause.
///
/// `event_rows.zig`'s `MAX_FAILURE_DETAIL_BYTES`. Applied at the WRITE rather
/// than at the read, so a runaway child cannot bloat the row — the cap bounds
/// the row, not the operator's visibility, since the console renders the cause
/// as one line anyway.
pub const MAX_FAILURE_DETAIL_BYTES: usize = 512;
