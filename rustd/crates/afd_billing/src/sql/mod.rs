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

/// Every tier one model is published at, across all its dates, oldest first.
///
/// `$1` provider · `$2` model name. The rater, not this statement, picks the
/// tier and the date, so one read serves every line of a piece of work.
pub const SELECT_PRICE_BOOK: &str = include_str!("select_price_book.sql");

/// The stored spellings of `billing.usage_lines.work_type` and
/// `billing.holds.work_type` (RULE STS: the schema names none of them).
pub mod work {
    /// A fleet run's lease.
    pub const FLEET_LEASE: &str = "fleet_lease";
    /// A setup-chat attempt.
    pub const AUTHORING_ATTEMPT: &str = "authoring_attempt";
}

/// The charges page's spellings for a fleet run's usage. Never stored: a
/// usage line names its work, and the page names the charge it reads as.
pub mod charge {
    /// A fleet run's usage, one row per usage line.
    pub const STAGE: &str = "stage";
}

/// The `billing.usage_lines.posture` values, as the resolver spells them.
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

/// The charges page's spelling for a setup chat's settlement, beside the two
/// [`charge`] types a fleet run writes. Never stored: it names the source.
pub const CHARGE_TYPE_AUTHORING: &str = "authoring";

/// The tenant's first charges page: newest first, no boundary.
///
/// Every charge is a usage line in `billing.usage_lines`, one row per line,
/// whether a fleet run or a setup chat logged it. The page is cut to its size
/// by the `(tenant_id, created_at, id)` index in a `MATERIALIZED` CTE BEFORE
/// any join, so the draft and fleet detail joins touch at most one page
/// however long the history is. A setup line names its fleet only through its
/// draft's saved fleet, which it may not have; a fleet line names its own.
/// `cached_input_token_count` is not selected: the charges response does not
/// carry it.
///
/// `$1` tenant · `$2` limit · `$3` [`CHARGE_TYPE_AUTHORING`] · `$4`
/// [`work::FLEET_LEASE`] · `$5` [`charge::STAGE`].
pub(crate) const SELECT_TENANT_CHARGES_FIRST: &str =
    include_str!("select_tenant_charges_first.sql");

/// A later charges page: strictly older than the cursor's boundary, the same
/// two sources cut the same way.
///
/// `$1` tenant · `$2` boundary instant · `$3` boundary id · `$4` limit · `$5`–`$7`
/// as `$3`–`$5` above.
pub(crate) const SELECT_TENANT_CHARGES_AFTER: &str =
    include_str!("select_tenant_charges_after.sql");
