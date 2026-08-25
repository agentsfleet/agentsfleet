//! `fleet.runner_affinity` and `fleet.runner_leases` — the claim, the fence,
//! and the row that records who owns a fleet's work.
//!
//! Text is byte-identical to `fleet/sql.zig` and `fleet/sql_lease_row.zig`.
//! That is not tidiness: row-equivalence is this milestone's cutover claim, the
//! dual-run differ that would have enforced it mechanically is gone with the
//! Zig lanes, and REVIEW reading these side by side against the originals is
//! what enforcement is left (Invariant 5). A statement is COPIED, never
//! re-derived — where a `$n` order looks odd it is odd upstream too.
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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use super::runner::Bound;

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
/// from zero. [`RESET_AFFINITY_METERS`] is what clears it, and only a FRESH
/// lease calls that.
///
/// Answers no row when a live runner still holds the slot — that absence is the
/// `.taken` verdict, not an error.
pub const CLAIM_AFFINITY_SLOT: &str = "\
INSERT INTO fleet.runner_affinity
  (fleet_id, last_runner_id, fencing_seq, leased_until,
   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, $4, $4)
ON CONFLICT (fleet_id) DO UPDATE
  SET last_runner_id = EXCLUDED.last_runner_id,
      fencing_seq    = fleet.runner_affinity.fencing_seq + 1,
      leased_until   = EXCLUDED.leased_until,
      updated_at     = EXCLUDED.updated_at
  WHERE fleet.runner_affinity.leased_until < $4
RETURNING fencing_seq";

/// Reset the slot's metering counters at the start of a fresh billing slice.
///
/// FRESH leases only. A reclaim must leave the cursor alone so the re-leased
/// run meters forward from where the dead holder stopped; the renewal CTE reads
/// this cursor for each slice's delta, so a stale value here would over-charge
/// the first renewal. That is why the caller treats a failed reset as a failed
/// lease issue rather than a warning — fail closed, never over-charge.
pub const RESET_AFFINITY_METERS: &str = "\
UPDATE fleet.runner_affinity
SET metered_input_tokens = 0, metered_cached_tokens = 0,
    metered_output_tokens = 0, last_metered_at = $2, updated_at = $2
WHERE fleet_id = $1::uuid";

/// Release the slot — fencing-guarded, so only the current holder can free it.
///
/// The guard is load-bearing rather than defensive: a holder superseded by a
/// reclaim would otherwise free the CURRENT holder's slot and hand one fleet to
/// two runners. Idempotent — a no-op when the row is gone or the token has been
/// bumped past this one.
pub const RELEASE_AFFINITY_SLOT: &str = "\
UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2
WHERE fleet_id = $1::uuid AND fencing_seq = $3";

/// Open a lease, record the event that opened it, and bump the runner's
/// lifetime acquired tally, atomically.
///
/// Writing the lease and its audit trail in one statement means an observer can
/// never see a lease with no corresponding event, or the reverse; the tally
/// rides the same statement so the acquired counter can never drift from the
/// rows it counts.
///
/// The lease stores no copy of the event body: the reclaim path reads it by
/// joining `core.fleet_events` on the `(fleet_id, event_id)` unique key, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_LEASE_WITH_EVENT: &str = "\
WITH inserted AS (
  INSERT INTO fleet.runner_leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id,
   actor, event_type, event_created_at,
   posture, provider, model,
   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
   fencing_token, lease_expires_at, status,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        $7, $8, $9, $10, $11, $12,
        0, 0, 0, $16,
        $13, $14, $15, $16, $16)
  RETURNING id, runner_id, fleet_id, event_id
), audit AS (
  INSERT INTO fleet.runner_events
    (id, runner_id, event_type, metadata, dedup_key, created_at)
  SELECT $17::uuid, runner_id, $18::text,
         jsonb_build_object($19::text, id::text, $20::text, fleet_id::text, $21::text, event_id, $22::text, $23::text),
         NULL, $16::bigint
  FROM inserted
  RETURNING id
)
INSERT INTO fleet.runner_lifetime_counters
  (runner_id, acquired, created_at, updated_at)
SELECT runner_id, 1, $16, $16
FROM inserted
ON CONFLICT (runner_id) DO UPDATE
   SET acquired = fleet.runner_lifetime_counters.acquired + 1,
       updated_at = EXCLUDED.updated_at";

/// Everything [`INSERT_LEASE_WITH_EVENT`] needs, by name.
///
/// Twenty-three positional parameters, `$16` referenced five times, and the
/// `VALUES` list mentioning `$13` after `$16` — the same shape, and the same
/// hazard, that [`super::runner::RegisterRow`] documents. Five of these are
/// same-typed text that a transposition would compile straight through, and
/// this workspace disables sqlx's `macros` feature deliberately, so there is no
/// compile-time query checking to catch it. Naming the fields is what replaces
/// it: the `$n` order is written ONCE, here, beside the text it orders.
#[derive(Debug)]
pub struct LeaseRow<'a> {
    /// The lease's durable identifier.
    pub lease_id: &'a Uuid7,
    /// The runner taking the work.
    pub runner_id: &'a Uuid7,
    /// The fleet whose slot was claimed.
    pub fleet_id: &'a Uuid7,
    /// The workspace the fleet belongs to.
    pub workspace_id: &'a Uuid7,
    /// The tenant whose wallet was gated and debited.
    pub tenant_id: &'a Uuid7,
    /// The event being leased. Text, not `uuid`: event ids are producer-shaped
    /// and the column takes them as written.
    pub event_id: &'a str,
    /// Who or what raised the event.
    pub actor: &'a str,
    /// The event's own type, carried so a reclaim need not re-read it.
    pub event_type: &'a str,
    /// When the event was raised, by the producer's clock.
    pub event_created_at: i64,
    /// The resolved billing posture, as its wire spelling.
    pub posture: &'a str,
    /// The provider resolved at billing.
    ///
    /// Stored alongside posture and model so the renew credit gate and the
    /// report settle can key the rate row by `(provider, model)` without
    /// re-resolving. Empty only on a reclaim, which carries the prior lease's
    /// billing instead.
    pub provider: &'a str,
    /// The model resolved at billing.
    pub model: &'a str,
    /// The claim's fencing token — the value every report is checked against.
    pub fencing_token: i64,
    /// When this lease stops being the live one.
    pub leased_until: i64,
    /// The status the row opens in.
    pub status: &'a str,
    /// Issue instant. Seeds `last_metered_at`, `created_at`, `updated_at`, and
    /// the audit row's `created_at` — one instant, so nothing in the family can
    /// disagree about when the lease began.
    pub now: UnixMillis,
    /// Identifier of the audit row this write also lands.
    pub event_row_id: &'a Uuid7,
    /// Whether this lease is a fresh pull or a reclaim, as its wire spelling.
    ///
    /// Reaches the audit row's metadata rather than a column: it explains the
    /// lease's provenance to an operator reading history, and nothing queries
    /// on it.
    pub kind: &'a str,
}

impl<'a> LeaseRow<'a> {
    /// Binds this row to [`INSERT_LEASE_WITH_EVENT`], in `$n` order.
    ///
    /// The four metadata keys (`$19`–`$22`) are constants rather than caller
    /// data, so they are supplied here — twenty-three binds, and none a caller
    /// has to place positionally.
    pub fn bind(&'a self) -> Bound<'a> {
        let millis = self.now.as_millis();
        sqlx::query(INSERT_LEASE_WITH_EVENT)
            .bind(self.lease_id.as_str())
            .bind(self.runner_id.as_str())
            .bind(self.fleet_id.as_str())
            .bind(self.workspace_id.as_str())
            .bind(self.tenant_id.as_str())
            .bind(self.event_id)
            .bind(self.actor)
            .bind(self.event_type)
            .bind(self.event_created_at)
            .bind(self.posture)
            .bind(self.provider)
            .bind(self.model)
            .bind(self.fencing_token)
            .bind(self.leased_until)
            .bind(self.status)
            .bind(millis)
            .bind(self.event_row_id.as_str())
            .bind(super::event_type::LEASE_ACQUIRED)
            .bind(super::meta::LEASE_ID)
            .bind(super::meta::FLEET_ID)
            .bind(super::meta::AGENTSFLEET_EVENT_ID)
            .bind(super::meta::KIND)
            .bind(self.kind)
    }
}

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
  RETURNING l.id, l.runner_id, l.fleet_id, l.event_id, l.actor, l.event_type,
            l.event_created_at, l.workspace_id, l.tenant_id,
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
       b.posture, b.model
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
/// The runner's labels (stored JSONB) bind as a constant `TEXT[]` via the
/// uncorrelated subquery, so `<@` stays a `column <@ constant` shape the
/// `required_tags` GIN index can serve — not a column-to-column join, which no
/// index serves.
///
/// `$1` active status, `$2` runner id, `$3` ready fleet ids, `$4` ceiling.
pub const SELECT_READY_CANDIDATES: &str = "\
SELECT z.id::text
FROM core.fleets z
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = z.id
WHERE z.status = $1
  AND z.id = ANY(($3::text[])::uuid[])
  AND z.required_tags <@ (
        SELECT COALESCE(array_agg(e), '{}'::text[])
        FROM jsonb_array_elements_text(
               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
                            THEN labels ELSE '[]'::jsonb END
                FROM fleet.runners WHERE id = $2::uuid)
             ) AS e
      )
ORDER BY (a.last_runner_id = $2::uuid) DESC NULLS LAST, z.created_at ASC
LIMIT $4";
