//! `fleet.runner_affinity` and `fleet.runner_leases` — the claim, the fence,
//! and the row that records who owns a fleet's work.
//!
//! Text began as a copy of the retired Zig daemon's `fleet/sql.zig` and
//! `fleet/sql_lease_row.zig`, which is why some `$n` orders look odd. The
//! candidate scan and the lease insert have since moved on, and each says how
//! beside its text.
//!
//! # The claim is the whole concurrency design
//!
//! [`CLAIM_AFFINITY_SLOT`] is one conditional UPSERT and it carries three jobs
//! at once: it wins the fleet iff the slot is free or its prior claim expired,
//! it bumps the monotonic `fencing_seq`, and it records the sticky-routing
//! hint. Exactly one of N racing runners takes the row. Crucially the claim
//! PRECEDES the event read, so a loser has consumed no event and nothing is
//! orphaned — which is why `test_lease_affinity_race` asserts one lease row and
//! one no-work reply rather than counting retries.

mod row;

pub use self::row::{INSERT_LEASE_WITH_EVENT, LeaseRow};

/// Claim a fleet's lease slot, bumping the monotonic fence.
///
/// `fleet_id` is the whole primary key (schema/630), so the conflict target IS
/// the table's only unique index — two runners racing a brand-new fleet's slot
/// take the update arm rather than colliding on an index this statement does
/// not name.
///
/// The durable metering cursor is seeded `0`/now on a brand-new slot and is
/// deliberately ABSENT from the `ON CONFLICT` SET, so it survives a reclaim:
/// the re-leased run meters forward from the dead holder's progress rather than
/// from zero. [`INSERT_LEASE_WITH_EVENT`]'s `held` arm is what clears it, and
/// only a FRESH lease sets the flag that arms it.
///
/// Answers no row when a live runner still holds the slot — that absence is the
/// `.taken` verdict, not an error.
///
/// # A held fleet is its holder's first
///
/// While `held_until` is in the future and its holder, `last_runner_id`, can
/// still lease — `$6` (active) and not degraded, and beaten within `$5`
/// (`RUNNER_OFFLINE_AFTER_MS`) — only the holder wins: its sandbox carries the
/// fleet's last run, and the next event should run there. A holder that
/// cannot lease binds nobody: a degraded runner is answered no work, and one
/// drained or revoked is refused before its poll or its beat reaches here.
/// A hold is a head start at the slot, never the slot. The holder still claims
/// through this statement and its fence, so one fleet still has one live
/// holder. The holder's own claim keeps `held_until`, so a claim that finds no
/// event leaves the hold in place for the next one; its report then records
/// the hold anew or clears it. Any other runner's claim clears it, since the
/// sandbox it named no longer carries the fleet's latest run.
///
/// `prior` reads the slot before the statement changes it, so the caller can
/// tell a claim on a held fleet from one on an unheld fleet. A CTE sees the
/// statement's starting snapshot on every Postgres version.
pub const CLAIM_AFFINITY_SLOT: &str = "\
WITH prior AS (
  SELECT held_until, last_runner_id FROM fleet.runner_affinity WHERE fleet_id = $1::uuid
)
INSERT INTO fleet.runner_affinity
  (fleet_id, last_runner_id, fencing_seq, leased_until,
   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, $4, $4)
ON CONFLICT (fleet_id) DO UPDATE
  SET last_runner_id = EXCLUDED.last_runner_id,
      fencing_seq    = fleet.runner_affinity.fencing_seq + 1,
      leased_until   = EXCLUDED.leased_until,
      held_until     = CASE WHEN fleet.runner_affinity.last_runner_id = EXCLUDED.last_runner_id
                            THEN fleet.runner_affinity.held_until END,
      updated_at     = EXCLUDED.updated_at
  WHERE fleet.runner_affinity.leased_until < $4
    AND (fleet.runner_affinity.held_until IS NULL
         OR fleet.runner_affinity.held_until <= $4
         OR fleet.runner_affinity.last_runner_id = $2::uuid
         OR NOT EXISTS (
              SELECT 1 FROM fleet.runners r
              WHERE r.id = fleet.runner_affinity.last_runner_id
                AND r.last_seen_at > $4 - $5
                AND r.admin_state = $6
                AND NOT r.degraded))
RETURNING fencing_seq,
          (SELECT held_until FROM prior),
          (SELECT last_runner_id::text FROM prior)";

/// Release the slot — fencing-guarded, so only the current holder can free it —
/// and record whether that holder keeps the fleet's sandbox: `$4` is when the
/// hold lapses, or null for none.
///
/// The guard is load-bearing rather than defensive: a holder superseded by a
/// reclaim would otherwise free the CURRENT holder's slot and hand one fleet to
/// two runners, and would record a hold on a sandbox no lease will reach.
/// Idempotent — a no-op when the row is gone or the token has been bumped past
/// this one.
pub const RELEASE_AFFINITY_SLOT: &str = "\
UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2, held_until = $4
WHERE fleet_id = $1::uuid AND fencing_seq = $3";

/// [`RELEASE_AFFINITY_SLOT`] for a claim that leased nothing.
///
/// It drops the sticky hint too, or a fleet whose pass keeps stopping sorts
/// first for that runner on every poll of its partition, starving the rest.
/// A fleet still held keeps its hint: the hint names the holder, and the hold
/// is what steers the fleet's next event there. Every fleet nobody holds is
/// unhinted as before, so the starvation guard still covers it.
pub const RELEASE_UNLEASED_SLOT: &str = "\
UPDATE fleet.runner_affinity
SET leased_until = $2, updated_at = $2,
    last_runner_id = CASE WHEN held_until > $2 THEN last_runner_id END
WHERE fleet_id = $1::uuid AND fencing_seq = $3";

/// Reclaim the fleet's latest `active` lease: find it, expire it, and return
/// what it was executing — in ONE statement.
///
/// Called only after a claim has been won, so the row it finds is unambiguously
/// the dead holder's. The single statement is what makes the find and the
/// expire inseparable: split in two, a concurrent sweep could expire the row
/// between them and two runners would re-lease the same event.
///
/// The `tally` CTE rides along because this is the sole `active` → `expired`
/// writer, so the lifetime counter can never drift from the rows it counts.
///
/// # The join is INNER on purpose
///
/// The body comes from `core.fleet_events` through the `(fleet_id, event_id)`
/// unique key rather than from a column on the lease — the hottest write in the
/// system does not duplicate the largest value in it. An event row deleted out
/// from under a live lease therefore yields NO row here, and the caller takes
/// fresh work instead of re-delivering an empty event. The status flip and the
/// tally still commit in that case: a data-modifying CTE runs to completion
/// whether or not the primary query reads its output, so the dead lease does
/// not linger `active`.
///
/// `$1` fleet id, `$2` the active status, `$3` the expired status, `$4` now.
pub const RECLAIM_PRIOR_ACTIVE: &str = "\
WITH bumped AS (
  UPDATE fleet.runner_leases AS l
  SET status = $3, updated_at = $4
  WHERE l.id = (
      SELECT id FROM fleet.runner_leases
      WHERE fleet_id = $1::uuid AND status = $2
      ORDER BY fencing_token DESC LIMIT 1
      FOR UPDATE
  )
  RETURNING l.id, l.runner_id, l.fleet_id, l.event_id, l.receipt, l.actor,
            l.event_type, l.event_created_at, l.workspace_id, l.tenant_id,
            l.posture, l.model
), tally AS (
  INSERT INTO fleet.runner_lifetime_counters
    (runner_id, expired, created_at, updated_at)
  SELECT runner_id, 1, $4, $4
  FROM bumped
  ON CONFLICT (runner_id) DO UPDATE
     SET expired = fleet.runner_lifetime_counters.expired + 1,
         updated_at = EXCLUDED.updated_at
)
SELECT b.id::text, b.event_id, b.actor, b.event_type, e.request_json::text,
       b.event_created_at, b.workspace_id::text, b.tenant_id::text,
       b.posture, b.model, b.receipt
FROM bumped b
JOIN core.fleet_events e
  ON e.fleet_id = b.fleet_id AND e.event_id = b.event_id";

/// Eligible active fleets for one lease poll, sticky-first and bounded.
///
/// Readiness NARROWS the input; it never decides eligibility. The label gate
/// and the sticky ordering are properties of this query — `required_tags <@
/// labels` still filters (empty tags are a subset of any labels, so any runner
/// qualifies) and the runner's own affinity still sorts to the front. The `$3`
/// membership restriction is the readiness index's contribution, and `$4` is
/// the ceiling that makes per-poll cost independent of how many fleets exist.
///
/// A slot a live runner holds is skipped: its claim would lose, and a fleet
/// running a long reply keeps its mark the whole time, so without `$5` every
/// poll that sampled it would spend a claim round trip finding that out. The
/// comparison is the claim's own, so a slot the claim could win is never
/// filtered. Ties break at random, so runners do not all race the oldest fleet
/// and no fleet whose pass keeps stopping is tried first on every poll.
///
/// The runner's labels (stored JSONB) bind as a constant `TEXT[]` via the
/// uncorrelated subquery, so `<@` stays a `column <@ constant` shape the
/// `required_tags` GIN index can serve — not a column-to-column join, which no
/// index serves.
///
/// A fleet another runner holds and can still lease is skipped the same way,
/// by the claim's own hold condition (see [`CLAIM_AFFINITY_SLOT`]).
///
/// `$1` active status, `$2` runner id, `$3` ready fleet ids, `$4` ceiling,
/// `$5` now, `$6` how long a silent runner stays live, `$7` the admin state
/// that may lease.
pub const SELECT_READY_CANDIDATES: &str = "\
SELECT z.id::text
FROM core.fleets z
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = z.id
WHERE z.status = $1
  AND z.id = ANY(($3::text[])::uuid[])
  AND (a.leased_until IS NULL OR a.leased_until < $5)
  AND (a.held_until IS NULL
       OR a.held_until <= $5
       OR a.last_runner_id = $2::uuid
       OR NOT EXISTS (
            SELECT 1 FROM fleet.runners r
            WHERE r.id = a.last_runner_id AND r.last_seen_at > $5 - $6
              AND r.admin_state = $7 AND NOT r.degraded))
  AND z.required_tags <@ (
        SELECT COALESCE(array_agg(e), '{}'::text[])
        FROM jsonb_array_elements_text(
               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
                            THEN labels ELSE '[]'::jsonb END
                FROM fleet.runners WHERE id = $2::uuid)
             ) AS e
      )
ORDER BY (a.last_runner_id = $2::uuid) DESC NULLS LAST, random()
LIMIT $4";

/// What one lease authorises a mint to reach.
///
/// Text from `http/handlers/runner/sql.zig`'s `SELECT_LEASE_SCOPE_FOR_MINT`,
/// with this crate's explicit casts. Every clause in the `WHERE` is an
/// authorisation, and the reason they are all in the statement is that the wire
/// carries no workspace at all: a prompt-injected child has nothing to forge,
/// because a `lease_id` that is foreign, expired or cancelled resolves to no
/// row rather than to another tenant's workspace.
///
/// - `l.runner_id = $2` — Invariant 2. The runner-id scope IS the ownership
///   check; the presenting bearer decides which leases exist.
/// - `l.status = $3 AND l.lease_expires_at > $4` — mint authority is bound to
///   the LEASE's lifetime, not the runner's, so a compromised runner replaying
///   a stale lease id cannot mint past the run it was issued for.
///
/// The fleet is joined rather than read second because the binding must come
/// from the fleet the lease authorised. Two statements could return a binding
/// belonging to a fleet the lease does not name.
///
/// `$1` lease, `$2` runner, `$3` active status, `$4` now.
pub const SELECT_LEASE_SCOPE_FOR_MINT: &str = "\
SELECT l.workspace_id::text, l.fleet_id::text, f.config_json::text, l.event_id
FROM fleet.runner_leases l
JOIN core.fleets f ON f.id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4";
