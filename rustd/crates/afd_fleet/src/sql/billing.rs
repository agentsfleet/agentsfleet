//! `billing.*` and the two `core` tables a money decision reads.
//!
//! Text is byte-identical to the Zig originals, which are scattered across
//! `state/sql.zig` (the wallet and the workspace→tenant hop),
//! `fleet/sql_budget_drain.zig` (the apportioning drain),
//! `state/fleet_telemetry_store.zig` (the ledger insert, written inline at its
//! call site) and `state/model_library/sql.zig` (the rate read, assembled there
//! from six `++` fragments). Collected here for the reason [`super`] gives:
//! REVIEW reading these side by side against the Zig is the ONLY enforcement of
//! row-equivalence, and a statement assembled from fragments in another file
//! cannot be read that way.
//!
//! Where a statement was built by concatenation upstream it is written out
//! flat here. That is not a rewrite — the bytes Postgres receives are the same
//! — and it is what makes the side-by-side read possible at all.

/// The tenant a workspace belongs to.
///
/// One hop, and it is the first thing the money path does: every ledger row,
/// wallet read and rate decision is keyed by tenant, and a lease carries only a
/// workspace. A miss is PERMANENT — a workspace with no tenant row is a broken
/// foreign key, not a transient fault — which is what
/// `resolveTenantFromWorkspace`'s `error.WorkspaceNotFound` distinguishes and
/// what the gate's refusal class turns on.
///
/// `$1` workspace.
pub const SELECT_TENANT_FOR_WORKSPACE: &str = "\
SELECT tenant_id::text
FROM core.workspaces
WHERE id = $1::uuid
LIMIT 1";

/// The tenant's credit pool.
///
/// `balance_exhausted_at` is selected but unread by the lease gate: the gate
/// compares the balance against an estimate and does not care when the pool ran
/// dry. It stays in the projection because the statement is shared with the
/// billing endpoint, and narrowing it here would fork one statement into two.
///
/// A tenant with NO wallet row is not a refusal — `getBilling` answers null and
/// `balanceCoversEstimate` admits. A tenant that has never been provisioned is
/// an operator fault, and refusing every one of its events would turn that into
/// an outage for a fleet that is otherwise healthy.
///
/// `$1` tenant.
pub const SELECT_TENANT_BALANCE: &str = "\
SELECT balance_nanos, updated_at, balance_exhausted_at
FROM billing.tenant_wallet
WHERE tenant_id = $1::uuid
LIMIT 1";

/// Credit drained by one fleet inside two windows, apportioned by run span.
///
/// The subtlest statement in the money path, and the comment above it in
/// `sql_budget_drain.zig` is longer than the SQL. The problem it solves: ONE
/// ledger row holds a whole run's accumulated spend, a run may last twelve
/// hours, and the daily window is a rolling twenty-four — so "which window does
/// this spend fall in" is not a question one timestamp can answer.
///
/// Each `CASE` is one row's share of the window opening at that floor: none if
/// the run stopped charging before it, all of it if the run began after it,
/// else the covered fraction of `[created_at, last_charged_at]`. `numeric`
/// because nanos times a millisecond span overflows `BIGINT`.
///
/// The floor test is `< floor` and not `<= floor`, matching the row filter's
/// `>=`, so a one-shot charge stamped exactly at a month start is counted
/// rather than dropped. `NULLIF` guards a future arm reordering rather than a
/// reachable divide: reaching the fraction arm implies `created_at < floor <=
/// last_charged_at`, which makes the divisor strictly positive.
///
/// The two arms are spelled out rather than factored into a helper. This is a
/// money path, and the SQL a reader sees here is the SQL that runs.
///
/// `$1` workspace, `$2` fleet, `$3` day floor, `$4` month floor, `$5` and `$6`
/// the two charge types counted.
pub const SELECT_BUDGET_DRAIN: &str = "\
SELECT
  COALESCE(SUM(CASE
    WHEN l.last_charged_at < $3::bigint THEN 0
    WHEN l.created_at >= $3::bigint THEN l.credit_deducted_nanos
    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $3::bigint)
         / NULLIF(l.last_charged_at - l.created_at, 0)
  END), 0)::bigint,
  COALESCE(SUM(CASE
    WHEN l.last_charged_at < $4::bigint THEN 0
    WHEN l.created_at >= $4::bigint THEN l.credit_deducted_nanos
    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $4::bigint)
         / NULLIF(l.last_charged_at - l.created_at, 0)
  END), 0)::bigint
FROM billing.usage_ledger l
WHERE l.workspace_id = $1::uuid AND l.fleet_id = $2::uuid
  AND l.charge_type IN ($5, $6)
  AND l.last_charged_at >= LEAST($3::bigint, $4::bigint)";

/// Record one charge.
///
/// `ON CONFLICT (event_id, charge_type) DO NOTHING` is the replay guard, and it
/// guards THIS ROW ONLY. The balance drain itself is not replay-guarded, which
/// is the entire reason the receive debit is gated on a first delivery — a
/// re-delivered event that reached this statement twice would write one row and
/// charge one balance, but a caller that skipped the delivery check would
/// charge the balance on every poll and leave one row to show for it.
///
/// `event_created_at` is the EVENT's instant, not this row's: every ledger row
/// for one event must carry the same value, and the receive row is written on a
/// different path at a different moment from the stage row a renewal
/// accumulates. A clock read here would disagree with the lease's copy whenever
/// the two straddle a millisecond.
///
/// `last_charged_at` equals `created_at` for a receive fee — it is charged once,
/// so its span is a point and the drain's apportioning degenerates to
/// all-or-nothing, which is what it has always been for this row.
///
/// `$1` row id, `$2` tenant, `$3` workspace, `$4` fleet, `$5` event, `$6` charge
/// type, `$7` posture, `$8` model, `$9` nanos, `$10`–`$13` token counts and wall
/// time, `$14` event instant, `$15` row instant, `$16` last-charged instant.
pub const INSERT_USAGE_LEDGER: &str = "\
INSERT INTO billing.usage_ledger
  (id, tenant_id, workspace_id, fleet_id, event_id,
   charge_type, posture, model,
   credit_deducted_nanos,
   token_count_input, token_count_cached_input, token_count_output, wall_ms,
   event_created_at, created_at, last_charged_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, $10,
        $11, $12, $13, $14, $15, $16)
ON CONFLICT (event_id, charge_type) DO NOTHING";

/// A model's rates, and the catalogue generation they were read at.
///
/// Assembled in the Zig from six `++` fragments; written flat here. Two
/// properties of its shape are load-bearing and neither is obvious:
///
/// The join is driven FROM the singleton revision row, not from the catalogue.
/// A `LEFT JOIN` this way round still yields the generation on one row when the
/// model is absent, where an inner join would return nothing and leave the
/// caller unable to tell "no such model" from "could not read the generation".
/// Those two get different treatment — one is `None`, the other fails closed.
///
/// One statement is one snapshot, so the generation and the row cannot skew.
/// Two statements would need an explicit transaction to claim as much, and a
/// caller that forgot one would cache a rate under a generation it does not
/// belong to — the exact failure the counter exists to prevent.
///
/// `$1` provider, `$2` model.
pub const LOAD_RATE_WITH_REVISION: &str = "\
SELECT r.revision, m.context_cap_tokens, m.input_nanos_per_mtok, \
m.cached_input_nanos_per_mtok, m.output_nanos_per_mtok
  FROM core.model_catalogue_revision r
  LEFT JOIN core.model_library m
    ON m.provider = $1 AND m.model_id = $2
 WHERE r.id = 1";

/// The `billing.usage_ledger.charge_type` values this crate writes and counts.
///
/// `fleet_telemetry_store.zig`'s `ChargeType`. Two spellings, declared once
/// (RULE UFS): the budget drain counts BOTH, so a charge type spelled
/// differently at the insert than at the drain is spend that never reaches a
/// ceiling.
pub mod charge {
    /// Charged when an event is admitted. Zero nanos today; the row is what
    /// the drain and the charges endpoint read.
    pub const RECEIVE: &str = "receive";

    /// Charged as a run proceeds — accumulated per renewal slice and settled at
    /// report. Written by §3, counted here.
    pub const STAGE: &str = "stage";
}

/// The `billing.usage_ledger.posture` values, as the resolver spells them.
///
/// `tenant_provider.zig`'s `Mode.label`. The column is written by this codebase
/// only, so an unknown spelling read back is a data-integrity fault to surface
/// rather than a value to guess at — which is what `Mode.parse` refuses to do
/// and what the previous per-file helpers got wrong, silently attributing every
/// unrecognised posture to `platform`.
pub mod posture {
    /// The platform supplies the provider key; token cost is charged here.
    pub const PLATFORM: &str = "platform";

    /// The tenant supplies their own key; tokens land on their provider bill
    /// and only the run fee is charged.
    pub const SELF_MANAGED: &str = "self_managed";
}
