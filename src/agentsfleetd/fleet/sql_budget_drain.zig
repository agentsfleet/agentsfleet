//! Budget-drain statement text, split from `sql.zig` for the file-length budget
//! (RULE SQLMOD stays intact: `sql.zig` re-exports both constants, so query text
//! remains grepable through one module and call sites keep their `sql.` spelling).
//!
//! One ledger row holds a whole run's accumulated spend, and a run may last
//! MAX_RUNTIME_MS (12h) against a rolling 24h window — so which window that spend
//! falls in is not a question one timestamp can answer. The retired
//! `metering_periods` table answered it with a row per slice; the ledger answers
//! it with the run's span, `[created_at, last_charged_at]`, and APPORTIONS the
//! total by how much of that span the window covers.
//!
//! Stamping the whole total on one instant instead would make the daily check
//! all-or-nothing: a 12h run whose first charge predates the floor would count
//! ZERO against a cap it had genuinely spent against, which under-enforces
//! exactly where the amounts are largest.
//!
//! Apportioning assumes spend is spread evenly across a run — true for the
//! time-based run fee, approximate for token cost. It is bounded by one run's
//! total either way, where the all-or-nothing error was unbounded in both
//! directions.

// Each CASE is one row's share of the window opening at that floor: none if the
// run stopped charging before it, all of it if the run began after it, else the
// covered fraction. `numeric` because nanos times a millisecond span overflows
// BIGINT. The two arms are spelled out rather than factored into a helper —
// this is a money path, and the SQL a reader sees here is the SQL that runs.
//
// The floor is INCLUSIVE, matching the row filter's `>=`: the first arm tests
// `< floor`, not `<= floor`, so a one-shot charge stamped exactly at the month
// start is counted rather than dropped. A run that stopped charging exactly at
// the floor still contributes nothing — it reaches the fraction arm and the
// numerator is zero — so the strict form loses no case the loose one caught.
// With `< floor` the fraction arm implies `created_at < floor <= last_charged_at`,
// which makes the divisor strictly positive; NULLIF stays as a guard against a
// future arm reordering, not against a reachable divide.

/// Drain totals at two window starts. `$3` and `$4` are the window instants.
/// No backdating parameter: `last_charged_at` states when a run stopped
/// charging, so the row filter is exact where the retired one was a heuristic.
pub const SELECT_BUDGET_DRAIN =
    \\SELECT
    \\  COALESCE(SUM(CASE
    \\    WHEN l.last_charged_at < $3::bigint THEN 0
    \\    WHEN l.created_at >= $3::bigint THEN l.credit_deducted_nanos
    \\    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $3::bigint)
    \\         / NULLIF(l.last_charged_at - l.created_at, 0)
    \\  END), 0)::bigint,
    \\  COALESCE(SUM(CASE
    \\    WHEN l.last_charged_at < $4::bigint THEN 0
    \\    WHEN l.created_at >= $4::bigint THEN l.credit_deducted_nanos
    \\    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $4::bigint)
    \\         / NULLIF(l.last_charged_at - l.created_at, 0)
    \\  END), 0)::bigint
    \\FROM billing.usage_ledger l
    \\WHERE l.workspace_id = $1::uuid AND l.fleet_id = $2::uuid
    \\  AND l.charge_type IN ($5, $6)
    \\  AND l.last_charged_at >= LEAST($3::bigint, $4::bigint)
;

/// The same drain, plus the fleet's declared budget, so the policy and the
/// spend it is checked against are read at one instant and cannot skew.
pub const SELECT_BUDGET_POLICY_AND_DRAIN =
    \\WITH drains AS (
    \\  SELECT l.credit_deducted_nanos AS amt,
    \\         l.created_at            AS first_at,
    \\         l.last_charged_at       AS last_at
    \\  FROM billing.usage_ledger l
    \\  WHERE l.workspace_id = $2::uuid AND l.fleet_id = $3::uuid
    \\    AND l.charge_type IN ($6, $7)
    \\    AND l.last_charged_at >= LEAST($4::bigint, $5::bigint)
    \\)
    \\SELECT
    \\  (z.config_json->'x-agentsfleet'->'budget')::text,
    \\  COALESCE((SELECT SUM(CASE
    \\    WHEN last_at < $4::bigint THEN 0
    \\    WHEN first_at >= $4::bigint THEN amt
    \\    ELSE amt::numeric * (last_at - $4::bigint) / NULLIF(last_at - first_at, 0)
    \\  END) FROM drains), 0)::bigint,
    \\  COALESCE((SELECT SUM(CASE
    \\    WHEN last_at < $5::bigint THEN 0
    \\    WHEN first_at >= $5::bigint THEN amt
    \\    ELSE amt::numeric * (last_at - $5::bigint) / NULLIF(last_at - first_at, 0)
    \\  END) FROM drains), 0)::bigint
    \\FROM core.fleets z
    \\WHERE z.id = $1::uuid
;
