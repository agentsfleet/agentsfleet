//! Every statement this crate runs: `billing.*`, and the `identity` and `model`
//! tables a money decision reads.
//!
//! The wallet and the workspace→tenant hop, the apportioning drain, the charge
//! insert, the rate read and the tenant's charges pages. The rest of the money
//! path sits beside its callers: the lease renewal and settle statements in
//! `afd_fleet/src/lease/sql/{renew,report}.rs` drain the balance and write the
//! `stage` charge rows, and `INSERT_WALLET` in `afd_tenant/src/sql/signup.rs`
//! opens a wallet with its starter grant.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.

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
pub const SELECT_TENANT_FOR_WORKSPACE: &str = include_str!("select_tenant_for_workspace.sql");

/// The tenant's credit pool.
///
/// `exhausted_at` is selected but unread by the lease gate: the gate compares
/// the balance against an estimate and does not care when the pool ran dry. It
/// stays in the projection because the statement is shared with the billing
/// endpoint ([`crate::tenant::Billing::snapshot`]), and narrowing it here would
/// fork one statement into two.
///
/// A tenant with NO wallet row is not a refusal — `getBilling` answers null and
/// `balanceCoversEstimate` admits. A tenant that has never been provisioned is
/// an operator fault, and refusing every one of its events would turn that into
/// an outage for a fleet that is otherwise healthy.
///
/// `$1` tenant.
pub const SELECT_WALLET: &str = include_str!("select_wallet.sql");

/// Credit drained by one fleet inside two windows, apportioned by run span.
///
/// The subtlest statement in the money path. The problem it solves: ONE
/// ledger row holds a whole run's accumulated spend, a run may last twelve
/// hours, and the daily window is a rolling twenty-four — so "which window does
/// this spend fall in" is not a question one timestamp can answer.
///
/// Each `CASE` is one row's share of the window opening at that floor: none if
/// the run stopped charging before it, all of it if the run began after it,
/// else the covered fraction of `[created_at, updated_at]`. A stage row's
/// `updated_at` is its last charge, since nothing but a charge updates the row.
/// The fraction is a ratio of `EXTRACT(EPOCH …)` spans, which Postgres answers
/// as `numeric`, so nanos times a span cannot overflow `BIGINT`.
///
/// The floor test is `< floor` and not `<= floor`, matching the row filter's
/// `>=`, so a one-shot charge stamped exactly at a month start is counted
/// rather than dropped. `NULLIF` guards a future arm reordering rather than a
/// reachable divide: reaching the fraction arm implies `created_at < floor <=
/// updated_at`, which makes the divisor strictly positive.
///
/// Both floors open from `now()`, the transaction's timestamp: the rolling
/// day is `now() - $3` with `$3` a whole day as a microsecond interval (so a
/// daylight-saving change in the session's zone cannot stretch it), cut to the
/// millisecond the `TIMESTAMPTZ(3)` charge columns hold — `now()` carries
/// microseconds, and an uncut floor would drop a charge stamped at it — and the
/// month is `date_trunc('month', now(), 'UTC')`. One instant feeds both, so the
/// day window can never open after the month window it is nested in, and a
/// caller holding a transaction measures against the same instant its other
/// statements see.
///
/// The two arms are spelled out rather than factored into a helper. This is a
/// money path, and the SQL a reader sees here is the SQL that runs.
///
/// `$1` workspace, `$2` fleet, `$3` the rolling day's span, `$4` and `$5` the
/// two charge types counted.
pub const SELECT_BUDGET_DRAIN: &str = include_str!("select_budget_drain.sql");

/// Record one charge.
///
/// `ON CONFLICT (event_id, charge_type, fleet_id) DO NOTHING` is the replay guard, and it
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
/// `updated_at` equals `created_at` for a receive fee: both are the statement's
/// `now()`, it is charged once, so its span is a point and the drain's
/// apportioning degenerates to all-or-nothing, which is what it has always been
/// for this row.
///
/// `$1` row id, `$2` tenant, `$3` workspace, `$4` fleet, `$5` event, `$6` charge
/// type, `$7` posture, `$8` model, `$9` nanos, `$10`–`$13` token counts and wall
/// time, `$14` event instant.
///
/// # `fleet_name` is read, never bound
///
/// It takes no parameter of its own — the subselect reads it from the fleet
/// `$4` already names. That is the difference between a snapshot and a claim: a
/// caller cannot pass a name that disagrees with the identifier beside it, and
/// no request field reaches this column. A fleet row purged before the charge
/// lands leaves the fleet's own identifier as the name, so the money still
/// lands and a reader tells it from a captured name by comparing it with
/// `fleet_id` on the same row.
pub const INSERT_FLEET_CHARGE: &str = include_str!("insert_fleet_charge.sql");

/// A platform model's three per-token prices.
///
/// No row is an authoritative miss: the catalogue does not carry the model,
/// which [`crate::Accounts::meter`] prices at the run fee alone. A datastore that
/// would not answer is an `Err`, so the two never collapse.
///
/// `$1` provider, `$2` model name.
pub const SELECT_PLATFORM_MODEL_PRICES: &str = include_str!("select_platform_model_prices.sql");

/// The `billing.fleet_charges.charge_type` values this crate writes and counts.
///
/// Two spellings, declared once (RULE UFS): the budget drain counts BOTH, so a charge type spelled
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

/// The `billing.fleet_charges.posture` values, as the resolver spells them.
///
/// The column is written by this codebase only, so an unknown spelling read
/// back is a data-integrity fault to surface rather than a value to guess at —
/// which is what [`crate::Posture::parse`] refuses to do. A guess would
/// silently attribute every unrecognised posture to `platform`.
pub mod posture {
    /// The platform supplies the provider key; token cost is charged here.
    pub const PLATFORM: &str = "platform";

    /// The tenant supplies their own key; tokens land on their provider bill
    /// and only the run fee is charged.
    pub const SELF_MANAGED: &str = "self_managed";
}

/// The tenant's first charges page: newest first, no boundary.
///
/// `fleet_charges.id` is table-qualified in the ORDER BY because the select
/// list emits `id::text`, which names an OUTPUT column `id`. A bare `ORDER BY
/// … id` resolves against the output list first and sorts by the text cast, an
/// ordering no index supplies. `cached_input_token_count` is not selected: the
/// charges response does not carry it.
///
/// `$1` tenant, `$2` limit.
pub(crate) const SELECT_TENANT_CHARGES_FIRST: &str =
    include_str!("select_tenant_charges_first.sql");

/// A later charges page: strictly older than the cursor's boundary.
///
/// `$1` tenant, `$2` boundary instant, `$3` boundary id, `$4` limit.
pub(crate) const SELECT_TENANT_CHARGES_AFTER: &str =
    include_str!("select_tenant_charges_after.sql");
