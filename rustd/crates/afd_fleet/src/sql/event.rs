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

/// The status a gate refusal flips an event INTO.
///
/// Daemon-side and terminal: a runner never reports it. Paired with
/// [`EVENT_STATUS_RECEIVED`] because the two are the halves of one predicate —
/// [`UPDATE_FLEET_EVENT_FAILURE`] guards on the second and writes the first.
pub const EVENT_STATUS_GATE_BLOCKED: &str = "gate_blocked";

/// End an event at a gate, naming what refused it.
///
/// Guarded on `status = $6` — always [`EVENT_STATUS_RECEIVED`] — so a terminal
/// row is NEVER reopened. A request arriving after a `gate_blocked` is a new
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

/// Why a gate refused an event, as `core.fleet_events.failure_label`.
///
/// One ownership site (RULE UFS) — the webhook path, the steer path and the
/// dashboard all read these strings, and a second spelling at a second write
/// site is a label that silently stops matching. `balance_exhausted`'s spelling
/// is pinned by `billing_and_provider_keys.md`.
pub mod label {
    /// The tenant's credit pool cannot cover the estimate.
    pub const BALANCE_EXHAUSTED: &str = "balance_exhausted";

    /// The workspace resolves to no tenant — a broken foreign key, not a blip.
    pub const TENANT_RESOLVE_FAILED: &str = "tenant_resolve_failed";

    /// The fleet's own declared ceiling is reached.
    ///
    /// Spelled identically to the runner-reported `budget_breach` failure
    /// class, which carries the same verdict for the mid-run kill — one label
    /// for two gates, so an operator greps one string whether the run was
    /// refused at issue or stopped in flight.
    pub const BUDGET_BREACH: &str = "budget_breach";

    /// A declared credential has no vault row.
    pub const SECRET_MISSING: &str = "secret_missing";

    /// A human refused the action.
    pub const APPROVAL_DENIED: &str = "approval_denied";

    /// A human was asked and the deadline passed.
    pub const APPROVAL_EXPIRED: &str = "approval_expired";

    /// The event names a type this daemon has no execution path for.
    ///
    /// New in the Rust port, and it has no Zig counterpart because the Zig
    /// carries `event_type` as a string all the way to the runner and never
    /// has to decide whether it can spell it. Here the wire type is a closed
    /// enum, so a producer from a newer build is a case that must be named —
    /// and naming it is the point: the alternative spellings available were
    /// all lies about what happened, and an operator reading
    /// `tenant_resolve_failed` on this row would go and look at billing.
    pub const EVENT_TYPE_UNSUPPORTED: &str = "event_type_unsupported";

    /// A write binding could not be turned into rules that bound anything.
    ///
    /// Also new, and also a fleet author's mistake rather than an operational
    /// fault: no repair branch was authorised, no base was named, or the
    /// binding covers more than the one repository the locked rules bound.
    /// Distinct from [`SECRET_MISSING`] because nothing is missing from the
    /// vault — the fleet's own configuration cannot be enforced.
    pub const BINDING_UNENFORCEABLE: &str = "binding_unenforceable";
}

/// The status a clean run ends in.
///
/// `event_rows.zig`'s `STATUS_PROCESSED`. Paired with
/// [`EVENT_STATUS_FLEET_ERROR`] and guarded against
/// [`EVENT_STATUS_RECEIVED`] — the three are the arms of one transition, so
/// they are declared together (RULE UFS).
pub const EVENT_STATUS_PROCESSED: &str = "processed";

/// The status a failed run ends in.
///
/// `event_rows.zig`'s `STATUS_FLEET_ERROR`. Runner-reported, unlike
/// [`EVENT_STATUS_GATE_BLOCKED`]: the daemon refuses at a gate, and the runner
/// reports a failure it observed.
pub const EVENT_STATUS_FLEET_ERROR: &str = "fleet_error";

/// The byte ceiling on a stored failure cause.
///
/// `event_rows.zig`'s `MAX_FAILURE_DETAIL_BYTES`. Applied at the WRITE rather
/// than at the read, so a runaway child cannot bloat the row — the cap bounds
/// the row, not the operator's visibility, since the console renders the cause
/// as one line anyway.
pub const MAX_FAILURE_DETAIL_BYTES: usize = 512;

/// The byte ceiling on a stored checkpoint response.
///
/// `event_rows.zig`'s `MAX_CHECKPOINT_RESPONSE_BYTES`. Four times
/// [`MAX_FAILURE_DETAIL_BYTES`] because this one is the resume cursor a session
/// continues from, where a cause line only has to be readable.
pub const MAX_CHECKPOINT_RESPONSE_BYTES: usize = 2048;

/// End an event with the runner's verdict.
///
/// The sibling of [`UPDATE_FLEET_EVENT_FAILURE`] and guarded the same way, on
/// `status = $9` — always [`EVENT_STATUS_RECEIVED`]. A terminal row is never
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

/// Checkpoint a fleet's session — one row per fleet, replaced in place.
///
/// `fleet/sql.zig`'s `UPSERT_FLEET_SESSION`. The cursor is the whole value: a
/// fleet resumes from `last_event_id` and `last_response`, so this row is what
/// makes a session continuous across runs on different runners.
///
/// Replaced rather than appended because only the LATEST checkpoint is ever
/// read. A history table here would grow per run and be queried never.
///
/// `$1` fleet, `$2` the context document, `$3` now.
pub const UPSERT_FLEET_SESSION: &str = "\
INSERT INTO core.fleet_sessions (fleet_id, context_json, checkpoint_at, created_at, updated_at)
VALUES ($1::uuid, $2::jsonb, $3, $3, $3)
ON CONFLICT (fleet_id) DO UPDATE
  SET context_json = EXCLUDED.context_json,
      checkpoint_at = EXCLUDED.checkpoint_at,
      updated_at = EXCLUDED.updated_at";
