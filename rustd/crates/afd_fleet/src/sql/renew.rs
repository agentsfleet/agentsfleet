//! The renewal: one statement that extends both deadline rows and meters the
//! slice between them, and the scoped read that precedes it.
//!
//! Text is byte-identical to `fleet/renewal.zig` and the inline load in
//! `fleet/service_renew.zig`.
//!
//! # Why BOTH rows move, in one statement
//!
//! Reclaimability and the kill deadline live on different rows.
//! `fleet.runner_affinity.leased_until` is what the claim checks, and
//! `fleet.runner_leases.lease_expires_at` is when the runner stops its child.
//! Advancing one without the other gets a healthy run reclaimed at the TTL
//! while its runner still believes it holds the lease — so both move to the
//! same clamped instant under the same fence, or neither does.
//!
//! A half-applied renewal is therefore a real outcome, not an impossibility:
//! the lease row can advance while a concurrent reclaim takes the slot between
//! the snapshot and the update's recheck. The caller reads that as LOST, which
//! kills the child cleanly, rather than as a renewal whose deadline it cannot
//! actually keep.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use super::runner::Bound;
use crate::money::Meter;

/// The lease a renewal is about, scoped to the runner presenting it.
///
/// `status` IS selected here, unlike [`super::report::SELECT_LEASE_FOR_REPORT`],
/// and the asymmetry is deliberate. A report has one refusal for a lease that
/// is not `active` — the fence — so reading the status early would only add a
/// second way to say it. A renewal has two, and they are different answers to
/// the runner: a lease that is no longer active is LOST and the child dies,
/// where a lease that is active but out of credit is a refusal it can report
/// against. Telling them apart needs the status before the money gates run.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_FOR_RENEW: &str = "\
SELECT tenant_id::text, fleet_id::text, workspace_id::text, posture, provider, model, status
FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid";

/// Extend both deadline rows and meter the slice, atomically.
///
/// `renewal.zig`'s `RENEW_METER_SQL`, copied. The CTE chain is
/// [`super::report::CLAIM_AND_SETTLE`]'s with two differences, and both are
/// about the cap:
///
/// `probe` computes `capped = LEAST(now + LEASE_TTL_MS, created_at +
/// MAX_RUNTIME_MS)` — the clamp Dimension 3.4 pins — and `guard` requires
/// `capped > now` on top of the fence. A run that has reached its hard ceiling
/// fails the guard, so it writes nothing and is not charged for asking.
///
/// The deltas are computed off the AFFINITY cursor rather than the lease's.
/// That row survives a reclaim, so a re-leased run meters forward from where
/// the dead holder stopped instead of re-charging the whole run — and a
/// re-sent renewal charges approximately zero, because the previous one already
/// advanced the cursor it is diffed against.
///
/// The trailing SELECT returns five columns rather than a verdict because the
/// three outcomes are not distinguishable by any one of them: `new_until`
/// present means renewed, but only if `aff_updated` is one; absent means
/// capped or lost, and only `hard_cap` against `now` separates those.
///
/// `$1` lease, `$2` runner, `$3` the unclamped want-until, `$4` max runtime,
/// `$5` the status guarded on, `$6` now, `$7`–`$9` cumulative tokens,
/// `$10`–`$13` the four rates, `$14` charge type, `$15` milliseconds per
/// second, `$16` tokens per mtok, `$17` ledger row id.
pub const RENEW_AND_METER: &str = "\
WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
           l.created_at, l.event_created_at,
           l.fencing_token, l.posture, l.model, a.fencing_seq,
           LEAST($3::bigint, l.created_at + $4::bigint) AS capped,
           GREATEST(0, $6::bigint - a.last_metered_at)         AS d_ms,
           GREATEST(0, $7::bigint - a.metered_input_tokens)    AS d_in,
           GREATEST(0, $8::bigint - a.metered_cached_tokens)   AS d_cached,
           GREATEST(0, $9::bigint - a.metered_output_tokens)   AS d_out
    FROM fleet.runner_leases l
    JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $5
    FOR UPDATE OF l, a
), bal AS (
    SELECT tb.tenant_id, tb.balance_nanos AS bal0
    FROM billing.tenant_wallet tb
    JOIN probe p ON p.tenant_id = tb.tenant_id
    FOR UPDATE OF tb
), calc AS (
    SELECT p.*, b.bal0,
           (d_ms * $10::bigint) / $15::bigint    AS run_fee,
           (d_in * $11::bigint) / $16::bigint
             + (d_cached * $12::bigint) / $16::bigint
             + (d_out * $13::bigint) / $16::bigint  AS token_cost
    FROM probe p
    LEFT JOIN bal b ON b.tenant_id = p.tenant_id
), guard AS (
    SELECT *, run_fee + token_cost AS slice,
           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged
    FROM calc
    WHERE fencing_token >= fencing_seq AND capped > $6::bigint
), ext_lease AS (
    UPDATE fleet.runner_leases l
    SET lease_expires_at = g.capped, updated_at = $6,
        metered_input_tokens = GREATEST(l.metered_input_tokens, $7),
        metered_cached_tokens = GREATEST(l.metered_cached_tokens, $8),
        metered_output_tokens = GREATEST(l.metered_output_tokens, $9),
        last_metered_at = $6
    FROM guard g WHERE l.id = g.id
    RETURNING g.capped, g.charged
), ext_aff AS (
    UPDATE fleet.runner_affinity a
    SET leased_until = g.capped, updated_at = $6,
        metered_input_tokens = GREATEST(a.metered_input_tokens, $7),
        metered_cached_tokens = GREATEST(a.metered_cached_tokens, $8),
        metered_output_tokens = GREATEST(a.metered_output_tokens, $9),
        last_metered_at = $6
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
), wallet AS (
    UPDATE billing.tenant_wallet tb
    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
        balance_exhausted_at = CASE
            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $6)
            ELSE NULL END,
        updated_at = $6
    FROM guard g WHERE tb.tenant_id = g.tenant_id
    RETURNING tb.tenant_id
), ledger AS (
    INSERT INTO billing.usage_ledger
      (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
       model, credit_deducted_nanos, token_count_input, token_count_cached_input,
       token_count_output, wall_ms, event_created_at, created_at, last_charged_at)
    SELECT $17::uuid, g.tenant_id, g.workspace_id, g.fleet_id, g.event_id, $14,
           g.posture, g.model, g.charged, g.d_in, g.d_cached, g.d_out, g.d_ms,
           g.event_created_at, $6, $6
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
)
SELECT
    (SELECT count(*) FROM probe)::bigint        AS probe_found,
    (SELECT capped FROM ext_lease)              AS new_until,
    (SELECT created_at + $4::bigint FROM probe) AS hard_cap,
    (SELECT count(*) FROM ext_aff)::bigint      AS aff_updated,
    (SELECT charged FROM ext_lease)             AS charged_nanos";

/// Everything [`RENEW_AND_METER`] needs, by name.
///
/// The sibling of [`super::report::SettleRow`], and grouped the same way: the
/// three counts and four rates arrive as one [`Meter`] rather than as seven
/// bare integers.
#[derive(Debug)]
pub struct RenewRow<'a> {
    /// The lease being renewed.
    pub lease_id: &'a str,
    /// The runner presenting the renewal — the ownership scope.
    pub runner_id: &'a Uuid7,
    /// The instant every row this statement writes is stamped with.
    pub now: UnixMillis,
    /// The deadline asked for, BEFORE the max-runtime clamp. The statement
    /// applies the clamp itself, against the lease's own `created_at`, so the
    /// ceiling is computed from the row rather than from the caller's memory
    /// of when the run began.
    pub want_until: UnixMillis,
    /// What this slice is priced from.
    pub meter: Meter,
    /// The ledger row this slice accumulates into.
    pub ledger_id: &'a Uuid7,
}

impl<'a> RenewRow<'a> {
    /// Binds this row to [`RENEW_AND_METER`], in `$n` order.
    ///
    /// The max runtime, the charge type, the guarded status and the two unit
    /// divisors are constants rather than caller data. `MAX_RUNTIME_MS` in
    /// particular: it is the platform's ceiling, not this request's, and a
    /// caller able to pass its own would be a caller able to renew past it.
    pub fn bind(&'a self) -> Bound<'a> {
        let millis = self.now.as_millis();
        let counts = self.meter.cumulative;
        let rates = self.meter.rates;
        sqlx::query(RENEW_AND_METER)
            .bind(self.lease_id)
            .bind(self.runner_id.as_str())
            .bind(self.want_until.as_millis())
            .bind(afd_core::timing::MAX_RUNTIME_MS)
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(millis)
            .bind(counts.input)
            .bind(counts.cached)
            .bind(counts.output)
            .bind(rates.run_nanos_per_sec)
            .bind(rates.input_nanos_per_mtok)
            .bind(rates.cached_input_nanos_per_mtok)
            .bind(rates.output_nanos_per_mtok)
            .bind(super::billing::charge::STAGE)
            .bind(crate::money::nanos::MS_PER_SEC)
            .bind(crate::money::nanos::TOKENS_PER_MTOK)
            .bind(self.ledger_id.as_str())
    }
}
