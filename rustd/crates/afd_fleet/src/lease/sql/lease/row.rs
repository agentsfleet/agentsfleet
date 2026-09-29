//! The lease row's one write: the lease, its audit event, the runner's tally
//! and a fresh lease's meter reset, fenced on the claim that earned it.
//!
//! Split from [`super`] when fencing the insert pushed that file past the
//! length cap; the statement and the struct that binds it stay together,
//! because the `$n` order is written once, beside the text it orders.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use afd_runner::sql::runner::Bound;

/// Open a lease, record the event that opened it, bump the runner's lifetime
/// acquired tally, and — on a fresh lease — reset the slot's metering cursor,
/// atomically.
///
/// Writing the lease and its audit trail in one statement means an observer can
/// never see a lease with no corresponding event, or the reverse; the tally
/// rides the same statement so the acquired counter can never drift from the
/// rows it counts.
///
/// # Fenced on the claim that earned it
///
/// The `held` arm writes nothing unless the slot still carries this claim's
/// token (`$13`) and has not lapsed (`$16`), and every other arm reads from it.
/// A pass that outlived its claim lost the slot to a runner that took the same
/// entry over, and an unfenced insert would have two runners executing one
/// event, the first also zeroing the second's meter. A lost fence writes no
/// row at all, which the caller reads as no-work.
///
/// `held` is an UPDATE rather than a guarded SELECT so the row lock and the
/// predicate are one step: a claim racing it either commits first and fails
/// the recheck, or waits for this insert and then reclaims the lease it wrote.
///
/// # The meter reset rides the insert
///
/// A FRESH lease zeroes the slot's cursor; a RECLAIM meters forward from the
/// dead holder's, and a stale cursor over-charges the first renewal. Here the
/// reset cannot fail apart from the lease: the fresh flag, `$25`, picks each
/// cursor column's new value inside `held`, and a reclaim writes them back
/// unchanged.
///
/// The lease stores no copy of the event body: the reclaim path reads it by
/// joining `core.fleet_events` on the `(fleet_id, event_id)` unique key, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_LEASE_WITH_EVENT: &str = "\
WITH held AS (
  UPDATE fleet.runner_affinity
  SET metered_input_tokens  = CASE WHEN $25::boolean THEN 0 ELSE metered_input_tokens END,
      metered_cached_tokens = CASE WHEN $25::boolean THEN 0 ELSE metered_cached_tokens END,
      metered_output_tokens = CASE WHEN $25::boolean THEN 0 ELSE metered_output_tokens END,
      last_metered_at       = CASE WHEN $25::boolean THEN $16 ELSE last_metered_at END,
      updated_at            = CASE WHEN $25::boolean THEN $16 ELSE updated_at END
  WHERE fleet_id = $3::uuid AND fencing_seq = $13 AND leased_until >= $16
  RETURNING fleet_id
), inserted AS (
  INSERT INTO fleet.runner_leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, receipt,
   actor, event_type, event_created_at,
   posture, provider, model,
   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
   fencing_token, lease_expires_at, status,
   created_at, updated_at)
  SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $24,
         $7, $8, $9, $10, $11, $12,
         0, 0, 0, $16,
         $13, $14, $15, $16, $16
  FROM held
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
