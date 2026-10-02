//! The statements the tool-call record verb and settlement run.
//!
//! Each one names `core.fleet_tool_call_details` (`schema/925`).

/// The lease a record post names, if this runner holds it live.
///
/// Answers the lease's own fencing token beside the fleet's live sequence, so
/// the caller can refuse a holder a reclaim has superseded. `LEFT JOIN`, as the
/// memory fence reads it: a lease whose fleet has no slot row is fenced by its
/// own token.
///
/// `$1` lease, `$2` runner, `$3` the active status, `$4` now.
pub const SELECT_LIVE_LEASE: &str = "\
SELECT l.fleet_id::text, l.workspace_id::text, l.event_id, l.fencing_token,
       COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4";

/// Holds the event row for the length of one post's transaction.
///
/// Two posts for one event would otherwise each read the budget before the
/// other wrote, and together keep more than it allows. `NO KEY UPDATE` is the
/// lock the settling `UPDATE` takes too, so a post and the settle of the same
/// event queue behind each other and nothing else does.
///
/// `$1` fleet, `$2` event.
pub const LOCK_EVENT: &str = "\
SELECT 1 FROM core.fleet_events
WHERE fleet_id = $1::uuid AND event_id = $2
FOR NO KEY UPDATE";

/// The record bytes one lease of an event already keeps, leaving out the
/// calls this post replaces.
///
/// `$1` fleet, `$2` event, `$3` fence, `$4` the call numbers being posted.
pub const SELECT_KEPT_BYTES: &str = "\
SELECT COALESCE(SUM(byte_count), 0)::bigint
FROM core.fleet_tool_call_details
WHERE fleet_id = $1::uuid AND event_id = $2 AND fencing_token = $3
  AND NOT (call_number = ANY($4::bigint[]))";

/// Writes a post's records in one statement, replacing any posted before.
///
/// The unique key is the upsert's arbiter, so a retried post rewrites the
/// rows it already wrote and adds none. `id` and `created_at` are the first
/// write's.
///
/// `$1` workspace, `$2` fleet, `$3` event, `$4` fence, `$5` now, then one
/// array per column: `$6` ids, `$7` call numbers, `$8` arguments as JSON text,
/// `$9` truncated arguments, `$10` outputs, `$11` line counts, `$12` truncated,
/// `$13` byte counts.
pub const UPSERT_RECORDS: &str = "\
INSERT INTO core.fleet_tool_call_details
  (id, workspace_id, fleet_id, event_id, fencing_token, call_number, arguments,
   truncated_arguments, output, output_line_count, truncated, byte_count,
   created_at, updated_at)
SELECT r.id, $1::uuid, $2::uuid, $3, $4, r.call_number, r.arguments::jsonb,
       r.truncated_arguments, r.output, r.output_line_count, r.truncated,
       r.byte_count, $5, $5
FROM UNNEST($6::uuid[], $7::bigint[], $8::text[], $9::bool[], $10::text[],
            $11::bigint[], $12::bool[], $13::bigint[])
  AS r(id, call_number, arguments, truncated_arguments, output,
       output_line_count, truncated, byte_count)
ON CONFLICT (fleet_id, event_id, fencing_token, call_number) DO UPDATE SET
  arguments = EXCLUDED.arguments,
  truncated_arguments = EXCLUDED.truncated_arguments,
  output = EXCLUDED.output,
  output_line_count = EXCLUDED.output_line_count,
  truncated = EXCLUDED.truncated,
  byte_count = EXCLUDED.byte_count,
  updated_at = EXCLUDED.updated_at";

/// Deletes the records every other lease of an event kept.
///
/// Run by the settling transaction: a reclaimed lease re-ran the event and
/// numbered its calls from 1 again, so only the settling fence's records
/// describe the run whose answer stands.
///
/// `$1` fleet, `$2` event, `$3` the settling fence.
pub const DELETE_OTHER_FENCES: &str = "\
DELETE FROM core.fleet_tool_call_details
WHERE fleet_id = $1::uuid AND event_id = $2 AND fencing_token <> $3";
