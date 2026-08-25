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
}
