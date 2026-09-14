//! The terminal report: one statement that claims the lease and settles the
//! money, and the scoped read that precedes it.
//!
//! Text is byte-identical to `fleet/renewal_settle.zig` and the inline load in
//! `fleet/service_report.zig`, for the reason [`super`] gives — REVIEW reading
//! these side by side against the Zig is the only enforcement of
//! row-equivalence left, and a statement re-derived rather than copied cannot
//! be read that way.
//!
//! # Why the claim and the settle are ONE statement
//!
//! They authorize each other. The fence that says this runner may report is the
//! same fence that says its final slice may be charged, and a reclaim that
//! bumps `fencing_seq` between the two would leave one of them done and the
//! other refused. Split in two, the cap path loses the last slice — the reclaim
//! wins the fence while the report is still pricing — and no amount of retrying
//! recovers it, because by then the lease is somebody else's.
//!
//! Fused, a racing reclaim blocks on the affinity row lock until this commits.
//! By the time it runs, the lease is `reported` and the slice is charged.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use afd_billing::Meter;
use afd_runner::sql::runner::Bound;

/// The lease a report is about, scoped to the runner presenting it.
///
/// The `runner_id` predicate IS the ownership check, and it is why a foreign
/// lease id and an unknown one answer the same 404: this statement cannot see
/// another runner's row, so the handler has nothing to leak. An id-only lookup
/// with an ownership comparison afterwards would put the two facts in different
/// places and make the endpoint an oracle for which lease ids are live.
///
/// `status` is deliberately ABSENT from the predicate. The claim below applies
/// it, and reading it here as well would let a lease flip between the two
/// statements and answer 404 for a row the claim would have fenced — two
/// different refusals for one race, decided by timing.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_FOR_REPORT: &str = "\
SELECT fleet_id::text, workspace_id::text, tenant_id::text,
       event_id, actor, posture, provider, model, fencing_token, receipt
FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid";

/// What a claim that matched no row MEANT, for the lease it named.
///
/// [`CLAIM_AND_SETTLE`] guards on `status = active`, so it answers the same
/// empty claim to two different situations: a holder the fleet superseded
/// while it ran, and this same runner re-sending a report it already settled
/// because the response was lost. The first is owed a refusal. The second is
/// owed the outcome it already has — a finished run whose answer is retried
/// into a permanent refusal is an answer thrown away, which is the failure
/// RULE IDMP names.
///
/// Scoped by `runner_id` for the reason [`SELECT_LEASE_FOR_REPORT`] gives: the
/// statement cannot see another runner's row, so it can leak nothing. Read
/// INSIDE the settle's transaction, on the snapshot the claim that missed
/// decided against — read outside it, a reclaim landing between the two would
/// answer about a lease the claim never saw.
///
/// The fence is deliberately absent. A settled report releases the fleet's
/// slot, and the next event's claim bumps `fencing_seq` past this runner's
/// token within milliseconds — so a fence comparison here would call a
/// correctly settled report superseded, which is precisely the answer this
/// statement exists to stop. The lease's own status is the fact that decides:
/// `reported` is this runner's row, already terminal.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_DISPOSITION: &str = "\
SELECT status FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid";

/// Claim the report and settle the final slice, atomically.
///
/// `renewal_settle.zig`'s `CLAIM_SETTLE_SQL`, copied. Eight CTEs, and the
/// ordering between them is the design:
///
/// `probe` reads the lease and the slot under `FOR UPDATE OF l, a` — that
/// affinity lock is what serialises a racing reclaim behind this statement —
/// and computes the four cursor deltas clamped at zero, so a runner reporting
/// counts BELOW the cursor charges nothing rather than a negative.
///
/// `bal` locks the wallet in a CTE of its own rather than riding `probe`'s
/// join: Postgres refuses `FOR UPDATE` on the nullable side of an outer join,
/// and the row has to stay optional because a tenant with no wallet still
/// reports. The lock makes `bal0` the LIVE balance after any wait.
///
/// `guard` is the fence, and every write below is `FROM guard` — so a
/// superseded holder writes nothing at all rather than some prefix of the six
/// writes. `charged = LEAST(slice, bal0)` is the wallet's real delta in every
/// interleaving, so audit rows at exhaustion sum to the actual drain and never
/// past it.
///
/// `claim` flips `active` → `reported` and advances the lease cursor;
/// `ext_aff` advances the slot's. Both clamp with `GREATEST(old, $n)`, so a
/// report carrying regressed cumulatives cannot rewind a cursor and hand the
/// next slice a delta it already charged for.
///
/// `ledger`'s `ON CONFLICT (event_id, charge_type) DO UPDATE` is the dedup the
/// spec's Dimension 3.3 names: a replayed report ACCUMULATES into the one
/// `stage` row rather than inserting a second, so the ledger stays at two rows
/// per event — one `receive`, one `stage` — however many times a report is
/// re-sent. It accumulates approximately zero, because `claim` already advanced
/// the cursors the deltas are measured from.
///
/// `tally` is gated `FROM claim` rather than `FROM guard`: the lifetime counter
/// must count leases actually flipped, so a fenced retry that claims nothing
/// also counts nothing.
///
/// `event_created_at` rides the probe's projection only so `ledger` can stamp
/// it. It is the EVENT's instant, shared by every row for that event, not this
/// settle's.
///
/// `$1` lease, `$2` runner, `$3` now, `$4`–`$6` cumulative tokens, `$7`–`$10`
/// the four rates, `$11` charge type, `$12` the status guarded on, `$13` the
/// status written, `$14` milliseconds per second, `$15` tokens per mtok,
/// `$16` ledger row id, `$17` whether the run succeeded.
pub const CLAIM_AND_SETTLE: &str = "\
WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
           l.event_created_at,
           l.posture, l.model, l.fencing_token, a.fencing_seq,
           GREATEST(0, $3::bigint - a.last_metered_at)         AS d_ms,
           GREATEST(0, $4::bigint - a.metered_input_tokens)    AS d_in,
           GREATEST(0, $5::bigint - a.metered_cached_tokens)   AS d_cached,
           GREATEST(0, $6::bigint - a.metered_output_tokens)   AS d_out
    FROM fleet.runner_leases l
    JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $12
    FOR UPDATE OF l, a
), bal AS (
    SELECT tb.tenant_id, tb.balance_nanos AS bal0
    FROM billing.tenant_wallet tb
    JOIN probe p ON p.tenant_id = tb.tenant_id
    FOR UPDATE OF tb
), calc AS (
    SELECT p.*, b.bal0,
           (d_ms * $7::bigint) / $14::bigint    AS run_fee,
           (d_in * $8::bigint) / $15::bigint
             + (d_cached * $9::bigint) / $15::bigint
             + (d_out * $10::bigint) / $15::bigint AS token_cost
    FROM probe p
    LEFT JOIN bal b ON b.tenant_id = p.tenant_id
), guard AS (
    SELECT *, run_fee + token_cost AS slice,
           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged
    FROM calc
    WHERE fencing_token >= fencing_seq
), claim AS (
    UPDATE fleet.runner_leases l
    SET status = $13,
        metered_input_tokens = GREATEST(l.metered_input_tokens, $4),
        metered_cached_tokens = GREATEST(l.metered_cached_tokens, $5),
        metered_output_tokens = GREATEST(l.metered_output_tokens, $6),
        last_metered_at = $3, updated_at = $3
    FROM guard g WHERE l.id = g.id
    RETURNING g.id
), ext_aff AS (
    UPDATE fleet.runner_affinity a
    SET metered_input_tokens = GREATEST(a.metered_input_tokens, $4),
        metered_cached_tokens = GREATEST(a.metered_cached_tokens, $5),
        metered_output_tokens = GREATEST(a.metered_output_tokens, $6),
        last_metered_at = $3, updated_at = $3
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
), wallet AS (
    UPDATE billing.tenant_wallet tb
    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
        balance_exhausted_at = CASE
            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $3)
            ELSE NULL END,
        updated_at = $3
    FROM guard g WHERE tb.tenant_id = g.tenant_id
    RETURNING tb.tenant_id
), ledger AS (
    INSERT INTO billing.usage_ledger
      (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
       model, credit_deducted_nanos, token_count_input, token_count_cached_input,
       token_count_output, wall_ms, event_created_at, created_at, last_charged_at)
    SELECT $16::uuid, g.tenant_id, g.workspace_id, g.fleet_id, g.event_id, $11,
           g.posture, g.model, g.charged, g.d_in, g.d_cached, g.d_out, g.d_ms,
           g.event_created_at, $3, $3
    FROM guard g
    ON CONFLICT (event_id, charge_type) DO UPDATE SET
        credit_deducted_nanos = billing.usage_ledger.credit_deducted_nanos
            + EXCLUDED.credit_deducted_nanos,
        token_count_input  = COALESCE(billing.usage_ledger.token_count_input, 0)
            + EXCLUDED.token_count_input,
        token_count_cached_input = COALESCE(billing.usage_ledger.token_count_cached_input, 0)
            + EXCLUDED.token_count_cached_input,
        token_count_output = COALESCE(billing.usage_ledger.token_count_output, 0)
            + EXCLUDED.token_count_output,
        wall_ms = COALESCE(billing.usage_ledger.wall_ms, 0) + EXCLUDED.wall_ms,
        last_charged_at = GREATEST(billing.usage_ledger.last_charged_at,
                                   EXCLUDED.last_charged_at)
    RETURNING event_id
), tally AS (
    INSERT INTO fleet.runner_lifetime_counters
      (runner_id, succeeded, failed, created_at, updated_at)
    SELECT $2::uuid,
           CASE WHEN $17::boolean THEN 1 ELSE 0 END,
           CASE WHEN $17::boolean THEN 0 ELSE 1 END,
           $3, $3
    FROM claim
    ON CONFLICT (runner_id) DO UPDATE
       SET succeeded = fleet.runner_lifetime_counters.succeeded + EXCLUDED.succeeded,
           failed    = fleet.runner_lifetime_counters.failed + EXCLUDED.failed,
           updated_at = EXCLUDED.updated_at
)
SELECT (SELECT charged FROM guard)          AS charged,
       (SELECT count(*) FROM claim)::bigint AS claimed";

/// Everything [`CLAIM_AND_SETTLE`] needs, by name.
///
/// Seventeen positional parameters, eleven of them `bigint`, and `$3`
/// referenced nine times — the shape [`super::lease::LeaseRow`] documents the
/// hazard of. Six of those parameters are the three token counts and the four
/// rates, which is where `renewal_settle.zig` splats a seven-field
/// `MeterInputs` of bare integers; here they arrive as one
/// [`Meter`](afd_billing::Meter), so a transposition has to get past two named
/// types instead of past nothing.
///
/// The `$n` order is written ONCE, in [`SettleRow::bind`], beside the text it
/// orders.
#[derive(Debug)]
pub struct SettleRow<'a> {
    /// The lease being reported on.
    pub lease_id: &'a str,
    /// The runner presenting the report — the ownership scope.
    pub runner_id: &'a Uuid7,
    /// The instant every row this statement writes is stamped with.
    pub now: UnixMillis,
    /// What the final slice is priced from: the run's cumulative counts, and
    /// the rates they are charged at.
    pub meter: Meter,
    /// The ledger row this settle writes, or accumulates into on a replay.
    pub ledger_id: &'a Uuid7,
    /// Whether the run finished cleanly — picks the tally column.
    pub succeeded: bool,
}

impl<'a> SettleRow<'a> {
    /// Binds this row to [`CLAIM_AND_SETTLE`], in `$n` order.
    ///
    /// The charge type, the two lease statuses and the two unit divisors are
    /// constants rather than caller data, so they are supplied here — seventeen
    /// binds, and none a caller has to place positionally. The divisors come
    /// from [`afd_billing`] rather than being written as literals, so the
    /// statement and the pure `slice_charge` reference cannot disagree about
    /// what a second or a million tokens is.
    pub fn bind(&'a self) -> Bound<'a> {
        let millis = self.now.as_millis();
        let counts = self.meter.cumulative;
        let rates = self.meter.rates;
        sqlx::query(CLAIM_AND_SETTLE)
            .bind(self.lease_id)
            .bind(self.runner_id.as_str())
            .bind(millis)
            .bind(counts.input)
            .bind(counts.cached)
            .bind(counts.output)
            .bind(rates.run_nanos_per_sec)
            .bind(rates.input_nanos_per_mtok)
            .bind(rates.cached_input_nanos_per_mtok)
            .bind(rates.output_nanos_per_mtok)
            .bind(afd_billing::sql::charge::STAGE)
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(super::LEASE_STATUS_REPORTED)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(afd_billing::nanos::TOKENS_PER_MTOK)
            .bind(self.ledger_id.as_str())
            .bind(self.succeeded)
    }
}

/// Record that a lease was given back.
///
/// `fleet/sql.zig`'s `INSERT_RUNNER_EVENT`, which the Zig reaches through
/// `runner_events.appendLeaseReleased`. The closing bracket of the
/// `lease_acquired` row [`super::lease::INSERT_LEASE_WITH_EVENT`] writes, and
/// deliberately a separate statement rather than a CTE on the settle: it is
/// best-effort audit, and a datastore blip writing history must not fail a
/// report whose money has already committed.
///
/// `$1` row id, `$2` runner, `$3` event type, `$4` now, `$5`–`$10` the three
/// metadata key/value pairs.
pub const INSERT_RUNNER_EVENT: &str = "\
INSERT INTO fleet.runner_events
  (id, runner_id, event_type, metadata, dedup_key, created_at)
VALUES ($1::uuid, $2::uuid, $3::text,
        jsonb_build_object($5::text, $6::text, $7::text, $8::text, $9::text, $10::text),
        NULL, $4::bigint)";

/// Owe the answer's delivery, in the report's own transaction.
///
/// The fifth write, and the one that closes the window Dimension 7.6 names: a
/// process dying between the result committing and the queue append used to
/// leave a run finished, charged and answered with nothing recording that a
/// delivery was owed. The row is that record, and it commits or rolls back with
/// the money, the result, the cursor and the slot.
///
/// `receipt` and `delivered_at` are left NULL on purpose. The queue append is
/// NOT part of this transaction and cannot be — no transaction spans PostgreSQL
/// and Dragonfly — so the producer appends afterwards and records the entry id
/// back here, exactly as `core.fleet_admissions` receipts an inbound append.
/// Until it does, this row IS the obligation and the producer's unreceipted
/// scan is what finds it.
///
/// `ON CONFLICT DO NOTHING` on the event, so a re-sent report owes one delivery
/// and not two. That agrees with the settle above, which answers a repeat with
/// `AlreadySettled` and charges nothing: both halves of a replayed report are
/// no-ops, which is what makes the endpoint idempotent rather than merely
/// idempotent about money.
///
/// `$1` row id, `$2` fleet, `$3` workspace, `$4` provider, `$5` event,
/// `$6` answer, `$7` now.
pub const OWE_DELIVERY: &str = "\
INSERT INTO core.fleet_obligations
  (id, fleet_id, workspace_id, provider, event_id, answer,
   receipt, delivered_at, attempt_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::text, $5::text, $6::text,
        NULL, NULL, 0, $7::bigint, $7::bigint)
ON CONFLICT ON CONSTRAINT uq_fleet_obligations_event DO NOTHING
RETURNING id";

/// Record the queue entry an obligation was appended to.
///
/// The third step of the same three the inbound producer takes — commit the
/// row, append the entry, record the receipt — and the one a crash is most
/// likely to miss, which is exactly why the producer's scan keys on
/// `receipt IS NULL` and not on anything the append itself wrote.
///
/// Guarded on the receipt still being NULL so a pass that races the fast path
/// cannot overwrite a receipt already recorded. The loser writes nothing and
/// its entry becomes a duplicate the worker acknowledges without delivering —
/// the safe direction, since the alternative is a row pointing at an entry
/// nobody is holding.
///
/// `$1` obligation row, `$2` receipt, `$3` now.
pub const RECEIPT_DELIVERY: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid AND receipt IS NULL";
