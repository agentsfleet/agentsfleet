//! Lease-row statement text, split from `sql.zig` (RULE SQLMOD stays intact:
//! `sql.zig` re-exports this, so query text remains grepable through one
//! module and call sites keep their `sql.` spelling).

/// Open a lease, record the event that opened it, and bump the runner's
/// lifetime acquired tally, atomically. Writing the lease and its audit trail
/// in one statement means an observer can never see a lease with no
/// corresponding event, or the reverse; the tally rides the same statement so
/// the acquired counter can never drift from the rows it counts.
pub const INSERT_LEASE_WITH_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runner_leases
    \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id,
    \\   actor, event_type, request_json, event_created_at,
    \\   posture, provider, model,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
    \\   fencing_token, lease_expires_at, status,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
    \\        $7, $8, $9, $10, $11, $12, $13,
    \\        0, 0, 0, $17,
    \\        $14, $15, $16, $17, $17)
    \\  RETURNING id, runner_id, fleet_id, event_id
    \\), audit AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\  SELECT $18::uuid, runner_id, $19::text, $17::bigint,
    \\         jsonb_build_object($20::text, id::text, $21::text, fleet_id::text, $22::text, event_id, $23::text, $24::text),
    \\         NULL, $17::bigint
    \\  FROM inserted
    \\  RETURNING id
    \\)
    \\INSERT INTO fleet.runner_lifetime_counters
    \\  (uid, runner_id, acquired, succeeded, failed, expired, created_at, updated_at)
    \\SELECT runner_id, runner_id, 1, 0, 0, 0, $17, $17
    \\FROM inserted
    \\ON CONFLICT (uid) DO UPDATE
    \\   SET acquired = fleet.runner_lifetime_counters.acquired + 1,
    \\       updated_at = EXCLUDED.updated_at
;
