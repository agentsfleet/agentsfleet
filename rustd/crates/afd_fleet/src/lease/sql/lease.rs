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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use afd_runner::sql::runner::Bound;

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
/// from zero. [`INSERT_LEASE_WITH_EVENT`]'s `reset` arm is what clears it, and
/// only a FRESH lease sets the flag that arms it.
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

/// Release the slot — fencing-guarded, so only the current holder can free it.
///
/// The guard is load-bearing rather than defensive: a holder superseded by a
/// reclaim would otherwise free the CURRENT holder's slot and hand one fleet to
/// two runners. Idempotent — a no-op when the row is gone or the token has been
/// bumped past this one.
pub const RELEASE_AFFINITY_SLOT: &str = "\
UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2
WHERE fleet_id = $1::uuid AND fencing_seq = $3";

/// Open a lease, record the event that opened it, bump the runner's lifetime
/// acquired tally, and — on a fresh lease — reset the slot's metering cursor,
/// atomically.
///
/// Writing the lease and its audit trail in one statement means an observer can
/// never see a lease with no corresponding event, or the reverse; the tally
/// rides the same statement so the acquired counter can never drift from the
/// rows it counts.
///
/// # The meter reset rides the insert
///
/// A FRESH lease starts a new billing slice, so the slot's cursor goes back to
/// zero; a RECLAIM leaves it, because the re-leased run meters forward from
/// where the dead holder stopped. The renewal CTE reads the cursor for each
/// slice's delta, so a lease issued over a stale one over-charges the first
/// renewal. In here the reset cannot fail apart from the lease — both land or
/// neither does — and an issue pays one round trip, not two. A data-modifying
/// CTE runs whether or not anything reads it, so the fresh flag, `$25`, is in
/// its `WHERE`: a reclaim's reset matches no row.
///
/// The lease stores no copy of the event body: the reclaim path reads it by
/// joining `core.fleet_events` on the `(fleet_id, event_id)` unique key, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_LEASE_WITH_EVENT: &str = "\
WITH reset AS (
  UPDATE fleet.runner_affinity
  SET metered_input_tokens = 0, metered_cached_tokens = 0,
      metered_output_tokens = 0, last_metered_at = $16, updated_at = $16
  WHERE fleet_id = $3::uuid AND $25::boolean
), inserted AS (
  INSERT INTO fleet.runner_leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, receipt,
   actor, event_type, event_created_at,
   posture, provider, model,
   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
   fencing_token, lease_expires_at, status,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $24,
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
/// Twenty-five positional parameters, `$16` referenced eight times, and the
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
    /// The event being leased — the admission ledger's LOGICAL id. Text, not
    /// `uuid`: event ids are ledger-shaped and the column takes them as
    /// written.
    pub event_id: &'a str,
    /// The stream entry this lease was handed, which is what its
    /// acknowledgement addresses. Distinct from [`Self::event_id`] because a
    /// replayed admission puts one logical event on two entries.
    pub receipt: &'a str,
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
    /// Whether this lease starts a new billing slice, which resets the slot's
    /// metering cursor in the same statement. True for a fresh lease only.
    pub reset_meters: bool,
}

impl<'a> LeaseRow<'a> {
    /// Binds this row to [`INSERT_LEASE_WITH_EVENT`], in `$n` order.
    ///
    /// The four metadata keys (`$19`–`$22`) are constants rather than caller
    /// data, so they are supplied here — twenty-five binds, and none a caller
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
            .bind(afd_runner::sql::event_type::LEASE_ACQUIRED)
            .bind(afd_runner::sql::meta::LEASE_ID)
            .bind(afd_runner::sql::meta::FLEET_ID)
            .bind(afd_runner::sql::meta::AGENTSFLEET_EVENT_ID)
            .bind(afd_runner::sql::meta::KIND)
            .bind(self.kind)
            .bind(self.receipt)
            .bind(self.reset_meters)
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
/// filtered. Ties break on a hash of fleet and runner rather than on age, so
/// runners polling one partition try different fleets first instead of all
/// racing the oldest.
///
/// The runner's labels (stored JSONB) bind as a constant `TEXT[]` via the
/// uncorrelated subquery, so `<@` stays a `column <@ constant` shape the
/// `required_tags` GIN index can serve — not a column-to-column join, which no
/// index serves.
///
/// `$1` active status, `$2` runner id, `$3` ready fleet ids, `$4` ceiling,
/// `$5` now.
pub const SELECT_READY_CANDIDATES: &str = "\
SELECT z.id::text
FROM core.fleets z
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = z.id
WHERE z.status = $1
  AND z.id = ANY(($3::text[])::uuid[])
  AND (a.leased_until IS NULL OR a.leased_until < $5)
  AND z.required_tags <@ (
        SELECT COALESCE(array_agg(e), '{}'::text[])
        FROM jsonb_array_elements_text(
               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
                            THEN labels ELSE '[]'::jsonb END
                FROM fleet.runners WHERE id = $2::uuid)
             ) AS e
      )
ORDER BY (a.last_runner_id = $2::uuid) DESC NULLS LAST, md5(z.id::text || $2::text)
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
