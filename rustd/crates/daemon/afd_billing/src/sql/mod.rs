//! Every statement this crate runs: `billing.*`, and the `identity` and `model`
//! tables a money decision reads.
//!
//! The wallet read, the rate reads and the tenant's charges pages. The rest of
//! the money path sits beside its callers: the run renewal and report
//! statements in `afd_fleet/src/run/sql/` and the ledger's `ledger/sql/` log
//! the usage lines, the `billing.usage_lines` triggers charge each line as it
//! is written, and `INSERT_WALLET` in `afd_tenant` opens a wallet with its
//! starter grant.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.

/// The tenant's credit pool, for the billing page
/// ([`crate::tenant::Billing::snapshot`]) and every funds check
/// ([`crate::ledger::balance`]).
///
/// A tenant with no wallet row has no row to return, and the caller refuses it
/// as the bootstrap-invariant violation it is.
///
/// `$1` tenant.
pub const SELECT_WALLET: &str = include_str!("select_wallet.sql");

/// Every coding tier one model is published at, across all its dates, oldest
/// first; decision quotes in the same table are left out.
///
/// `$1` provider · `$2` model name. The rater, not this statement, picks the
/// tier and the date, so one read serves every line of a piece of work.
pub const SELECT_PRICE_BOOK: &str = include_str!("select_price_book.sql");

/// Active tiers at the native call instant, using the frozen model/revision.
/// `$1` model UUID · `$2` reviewed revision UUID · `$3` native call timestamp.
pub const SELECT_DECISION_PRICE_BOOK: &str = include_str!("select_decision_price_book.sql");

/// The stored spellings of `billing.usage_lines.work_type` (RULE STS: the
/// schema names none of them).
pub mod work {
    /// A fleet run's run.
    pub const FLEET_RUN: &str = "fleet_run";
}

/// The charges page's spelling for a fleet run's usage. Never stored: a usage
/// line names its work, and the page names the charge it reads as.
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

/// The tenant's first charges page: newest first, no boundary.
///
/// Every charge is a usage line in `billing.usage_lines`, one row per line,
/// read by the `(tenant_id, created_at, id)` index. A line names its own fleet,
/// so the page joins nothing. `cached_input_token_count` is not selected: the
/// charges response does not carry it.
///
/// `$1` tenant · `$2` limit · `$3` [`charge::STAGE`].
pub(crate) const SELECT_TENANT_CHARGES_FIRST: &str =
    include_str!("select_tenant_charges_first.sql");

/// A later charges page: strictly older than the cursor's boundary.
///
/// `$1` tenant · `$2` boundary instant · `$3` boundary id · `$4` limit · `$5`
/// [`charge::STAGE`].
pub(crate) const SELECT_TENANT_CHARGES_AFTER: &str =
    include_str!("select_tenant_charges_after.sql");
