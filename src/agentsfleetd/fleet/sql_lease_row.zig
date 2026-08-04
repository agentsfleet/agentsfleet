//! Lease-row statement text, split from `sql.zig` (RULE SQLMOD stays intact:
//! `sql.zig` re-exports this, so query text remains grepable through one
//! module and call sites keep their `sql.` spelling).

/// Open a lease, record the event that opened it, and bump the runner's
/// lifetime acquired tally, atomically. Writing the lease and its audit trail
/// in one statement means an observer can never see a lease with no
/// corresponding event, or the reverse; the tally rides the same statement so
/// the acquired counter can never drift from the rows it counts.
///
/// The lease stores no copy of the event body: `fleet/reclaim.zig` reads it by
/// joining `core.fleet_events` on the `(fleet_id, event_id)` unique key, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_LEASE_WITH_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runner_leases
    \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id,
    \\   actor, event_type, event_created_at,
    \\   posture, provider, model,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
    \\   fencing_token, lease_expires_at, status,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
    \\        $7, $8, $9, $10, $11, $12,
    \\        0, 0, 0, $16,
    \\        $13, $14, $15, $16, $16)
    \\  RETURNING id, runner_id, fleet_id, event_id
    \\), audit AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, metadata, dedup_key, created_at)
    \\  SELECT $17::uuid, runner_id, $18::text,
    \\         jsonb_build_object($19::text, id::text, $20::text, fleet_id::text, $21::text, event_id, $22::text, $23::text),
    \\         NULL, $16::bigint
    \\  FROM inserted
    \\  RETURNING id
    \\)
    \\INSERT INTO fleet.runner_lifetime_counters
    \\  (runner_id, acquired, created_at, updated_at)
    \\SELECT runner_id, 1, $16, $16
    \\FROM inserted
    \\ON CONFLICT (runner_id) DO UPDATE
    \\   SET acquired = fleet.runner_lifetime_counters.acquired + 1,
    \\       updated_at = EXCLUDED.updated_at
;
