//! The statements the background sweepers run.
//!
//! Copied from `fleet/sql.zig`, where the same four serve the liveness sweeper.
//! They live in their own module rather than beside the runner store's for the
//! reason [`super`] gives: the split is by CALLER, and nothing on a request
//! path runs any of these.
//!
//! # Every one of them is bounded and idempotent
//!
//! A sweeper's pass can fail halfway and be re-run, so each statement either
//! changes nothing on a second pass (the offline event's dedup key, the drained
//! transition's state predicate) or converges (the slot expiry's `>` guard).
//! That is what lets [`crate::sweep::run`] report a failure and carry on rather
//! than having to unwind one.

/// The runners one liveness pass considers.
///
/// Three disjunctions, and each is a different reason a runner needs looking
/// at: it has gone quiet, it is draining, or it is not active and still holds
/// leases. A runner that is none of these is healthy and is not fetched at all
/// — which is what keeps the pass proportional to the trouble rather than to
/// the fleet.
///
/// `ORDER BY r.updated_at ASC, r.id ASC` with a LIMIT is the pass's bound: the
/// least recently touched runners are swept first, so no runner can be starved
/// by a busier neighbour, and `id` settles a same-millisecond tie so the order
/// is total.
///
/// `$1` the never-seen sentinel, `$2` now, `$3` the offline threshold,
/// `$4` the active admin state, `$5` the active lease status, `$6` draining,
/// `$7` the batch limit.
pub const SELECT_DUE_RUNNERS: &str = "\
SELECT r.id::text, r.last_seen_at, r.admin_state
FROM fleet.runners r
WHERE (r.last_seen_at <> $1 AND ($2::bigint - r.last_seen_at) > $3)
   OR r.admin_state = $6
   OR (r.admin_state <> $4 AND EXISTS (
        SELECT 1 FROM fleet.runner_leases l
        WHERE l.runner_id = r.id AND l.status = $5
      ))
ORDER BY r.updated_at ASC, r.id ASC
LIMIT $7";

/// Records a runner going offline, at most once per stale episode.
///
/// The partial unique index on `(runner_id, dedup_key)` is what makes the sweep
/// idempotent: a cycle that re-observes the same stale runner inserts nothing,
/// and the returned count tells the caller whether THIS pass was the one that
/// recorded it. Without it, a runner offline for an hour would write a row
/// every ten seconds.
///
/// `$1` event id, `$2` runner, `$3` event type, `$4` now, `$5` the metadata
/// key, `$6` the last-seen instant — which is ALSO the dedup key, so one stale
/// episode is one row.
pub const INSERT_OFFLINE_EVENT: &str = "\
WITH inserted AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  VALUES ($1::uuid, $2::uuid, $3::text,
          jsonb_build_object($5::text, $6::bigint), $6::bigint, $4::bigint)
  ON CONFLICT (runner_id, dedup_key)
    WHERE event_type = 'runner_offline' AND dedup_key IS NOT NULL
  DO NOTHING
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM inserted";

/// Releases the affinity slots a dead runner still holds.
///
/// A fleet's slot is what stops two runners racing for the same work, so a
/// runner that died holding one would keep that fleet unrunnable until the
/// lease TTL elapsed. The `leased_until > $3` guard is what makes it converge:
/// a slot already released is not released again.
///
/// `$1` runner, `$2` the active lease status, `$3` the instant to expire at,
/// `$4` now.
pub const EXPIRE_ACTIVE_LEASE_SLOTS: &str = "\
WITH expired AS (
  UPDATE fleet.runner_affinity a
  SET leased_until = $3, updated_at = $4
  WHERE a.last_runner_id = $1::uuid
    AND a.leased_until > $3
    AND a.fleet_id IN (
      SELECT l.fleet_id FROM fleet.runner_leases l
      WHERE l.runner_id = $1::uuid AND l.status = $2
    )
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM expired";

/// Finishes draining a runner once its last lease is gone, and records the
/// transition in the same statement.
///
/// The `NOT EXISTS` guard is the safety property: a draining runner still
/// holding a lease is still working, and marking it drained would tell an
/// operator it was safe to take the host away mid-run. The event is written in
/// the same statement as the state change, so history cannot disagree with the
/// row it describes.
///
/// `$1` runner, `$2` drained, `$3` now, `$4` draining, `$5` the active lease
/// status, `$6` event id, `$7` event type, `$8` the from-state key, `$9` the
/// to-state key.
pub const MARK_DRAINED_IF_IDLE: &str = "\
WITH updated AS (
  UPDATE fleet.runners r
  SET admin_state = $2, updated_at = $3
  WHERE r.id = $1::uuid AND r.admin_state = $4
    AND NOT EXISTS (
      SELECT 1 FROM fleet.runner_leases l
      WHERE l.runner_id = r.id AND l.status = $5
    )
  RETURNING r.id
), inserted AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $6::uuid, id, $7::text,
         jsonb_build_object($8::text, $4::text, $9::text, $2::text), NULL, $3::bigint
  FROM updated
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM inserted";

/// The next page of active fleets, after the cursor's position.
///
/// A KEYSET cursor, not an offset: `(updated_at, id) > ($2, $3)` resumes
/// exactly where the last page stopped, so successive passes advance through
/// the population instead of re-reading its head. The shape this replaced —
/// ordered and limited with no cursor — read the same hundred fleets every
/// pass and never reached the rest, which made the recovery bound infinite for
/// any deployment with more active fleets than one batch.
///
/// The composite comparison is what makes it total: `updated_at` alone repeats
/// across fleets touched in the same millisecond, and a cursor that skipped
/// them would drop fleets rather than revisit them.
///
/// `$1` the active status, `$2` the cursor's instant, `$3` the cursor's id,
/// `$4` the batch limit.
pub const SELECT_ACTIVE_FLEETS_AFTER: &str = "\
SELECT id::text, updated_at FROM core.fleets
WHERE status = $1 AND (updated_at, id) > ($2::bigint, $3::uuid)
ORDER BY updated_at ASC, id ASC
LIMIT $4";

/// One batch of settled leases past the retention window.
///
/// `updated_at`, not `created_at`, is the retention clock: settle and reclaim
/// both stamp it, so the window counts from the settlement the API documents.
///
/// `FOR UPDATE SKIP LOCKED` because every replica runs its own sweeper. Without
/// it a second replica blocks on the first's row locks and then deletes
/// nothing, paying the full search cost for zero work; with it, concurrent
/// sweepers take disjoint batches.
///
/// `$1` the terminal statuses, `$2` the cutoff, `$3` the batch limit.
pub const DELETE_TERMINAL_LEASES_BATCH: &str = "\
DELETE FROM fleet.runner_leases
WHERE id IN (
  SELECT id FROM fleet.runner_leases
  WHERE status = ANY($1::text[]) AND updated_at < $2
  LIMIT $3
  FOR UPDATE SKIP LOCKED
)";

/// One batch of per-work event rows past the retention window.
///
/// Per-work tags only. The LIFECYCLE tags — enrolment, going offline, draining
/// — are never eligible, because they are the record of what a host did over
/// its whole life and an operator reading a six-month-old incident needs them.
///
/// `$1` the deletable event types, `$2` the cutoff, `$3` the batch limit.
pub const DELETE_AGED_RUNNER_EVENTS_BATCH: &str = "\
DELETE FROM fleet.runner_events
WHERE id IN (
  SELECT id FROM fleet.runner_events
  WHERE event_type = ANY($1::text[]) AND created_at < $2
  LIMIT $3
  FOR UPDATE SKIP LOCKED
)";

/// One batch of abandoned `active` leases, flipped to `expired`.
///
/// The rows nothing will ever settle: a fleet whose events stopped arriving is
/// never re-leased, so the reclaim path never reaches its stranded lease. What
/// makes an age-keyed flip safe here is the renewal CEILING — a lease anything
/// still holds is at most `MAX_RUNTIME_MS` stale, and the retention window is
/// longer than that, so a row past the cutoff is provably abandoned.
///
/// The tally rides the same statement as the flip, so the counter cannot drift
/// from the rows it describes. Grouped by runner because one batch can carry
/// several of a runner's leases, which is why the conflict clause ADDS the
/// batch's count rather than one.
///
/// `$1` the active status, `$2` the cutoff, `$3` expired, `$4` the batch limit,
/// `$5` now.
pub const EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH: &str = "\
WITH doomed AS (
  SELECT id, runner_id FROM fleet.runner_leases
  WHERE status = ANY($1::text[]) AND updated_at < $2
  LIMIT $4
  FOR UPDATE SKIP LOCKED
), tally AS (
  INSERT INTO fleet.runner_lifetime_counters
    (runner_id, expired, created_at, updated_at)
  SELECT d.runner_id, COUNT(*)::bigint, $5, $5
  FROM doomed d GROUP BY d.runner_id
  ON CONFLICT (runner_id) DO UPDATE
     SET expired = fleet.runner_lifetime_counters.expired + EXCLUDED.expired,
         updated_at = EXCLUDED.updated_at
)
UPDATE fleet.runner_leases AS l
SET status = $3, updated_at = $5
FROM doomed d
WHERE l.id = d.id";

/// One batch of repair verifications whose wait is over, claimed for dispatch.
///
/// Text from `state/repair_sql.zig`. Every clause is load-bearing:
///
/// - `verifier_event_id IS NULL` — the intent has not yet produced an event.
///   That column is what makes the whole loop idempotent: once it is set, this
///   statement can never select the row again.
/// - `verify_after <= $1` — the deployment has had its settling time.
/// - `dispatch_claim_token IS NULL OR dispatch_claimed_at <= $2` — either
///   nobody holds it, or whoever did has been gone longer than a claim lives.
///   A dispatcher that died mid-flight therefore releases its work by ELAPSING
///   rather than by anything having to notice it died.
/// - The `NOT EXISTS` over sibling links — a commit that arrived through two
///   pull requests is ambiguous about which repair to credit, so it is left
///   alone rather than attributed by guess.
///
/// `FOR UPDATE OF v SKIP LOCKED` because every replica runs its own dispatcher:
/// concurrent passes take disjoint batches instead of queueing behind one
/// another. The claim and the read are ONE statement, so no row can be selected
/// as due and then claimed by somebody else before this pass writes its token.
///
/// `$1` now, `$2` the stale-claim cutoff, `$3` the batch limit, `$4` this
/// pass's claim token.
pub const CLAIM_DUE_REPAIR_VERIFICATIONS: &str = "\
WITH due AS (
  SELECT v.id
  FROM core.repair_verifications v
  JOIN core.repair_pr_links l ON l.id = v.repair_link_id
  WHERE v.verifier_event_id IS NULL
    AND v.verify_after <= $1
    AND (v.dispatch_claim_token IS NULL OR v.dispatch_claimed_at <= $2)
    AND NOT EXISTS (
      SELECT 1 FROM core.repair_pr_links other_link
      WHERE other_link.workspace_id = l.workspace_id
        AND lower(other_link.repository) = lower(l.repository)
        AND other_link.merged_commit_sha = l.merged_commit_sha
        AND other_link.id <> l.id)
  ORDER BY v.verify_after ASC, v.id ASC
  FOR UPDATE OF v SKIP LOCKED
  LIMIT $3
), claimed AS (
  UPDATE core.repair_verifications v
  SET dispatch_claim_token = $4::uuid, dispatch_claimed_at = $1,
      dispatch_attempts = v.dispatch_attempts + 1, updated_at = $1
  FROM due WHERE v.id = due.id
  RETURNING v.*)
SELECT v.id::text, v.repair_link_id::text, l.repository,
       v.workspace_id::text, v.verifier_fleet_id::text,
       l.fleet_id::text, l.event_id, e.request_json::text,
       COALESCE(e.response_text, ''),
       l.pr_number, l.pr_url, l.merged_commit_sha, l.merged_at,
       p.provider, p.provider_deployment_id, p.conclusion, p.completed_at,
       v.verify_after
FROM claimed v
JOIN core.repair_pr_links l ON l.id = v.repair_link_id
JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
JOIN core.repair_production_results p ON p.id = v.production_result_id
ORDER BY v.verify_after ASC, v.id ASC";

/// Records which event an intent produced, releasing the claim.
///
/// Guarded on the claim token AND on the event still being absent, so a
/// dispatcher whose claim has already lapsed — a slow pass whose work another
/// replica has since redone — writes nothing rather than overwriting the event
/// id that replica recorded.
///
/// `$1` verification, `$2` the claim token, `$3` the event id, `$4` now.
pub const COMPLETE_REPAIR_VERIFICATION: &str = "\
UPDATE core.repair_verifications
SET verifier_event_id = $3, dispatch_claim_token = NULL,
    dispatch_claimed_at = NULL, updated_at = $4
WHERE id = $1::uuid AND dispatch_claim_token = $2::uuid
  AND verifier_event_id IS NULL";

/// The completed intents whose append-once key is still in Redis.
///
/// Cleared only after the durable link exists, which is why this reads
/// `verifier_event_id IS NOT NULL`: forgetting the key any earlier would let a
/// retry append a second event, which is the duplicate the key exists to
/// prevent.
///
/// `$1` the cutoff, `$2` the batch limit.
pub const SELECT_REPAIR_VERIFICATION_CLEANUP: &str = "\
SELECT id::text
FROM core.repair_verifications
WHERE verifier_event_id IS NOT NULL
  AND redis_once_key_cleared_at IS NULL
  AND updated_at <= $1
ORDER BY updated_at ASC, id ASC
LIMIT $2";

/// Marks a batch of append-once keys as forgotten.
///
/// One statement for the whole batch, keyed by a JSON array of identifiers, so
/// a page of cleanups costs one round trip rather than one per row.
///
/// `$1` the identifiers, `$2` now.
pub const COMPLETE_REPAIR_VERIFICATION_CLEANUP: &str = "\
UPDATE core.repair_verifications
SET redis_once_key_cleared_at = $2, updated_at = $2
WHERE id IN (
  SELECT value::uuid FROM jsonb_array_elements_text($1::jsonb)
)";
