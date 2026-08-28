//! Every statement this crate runs, collected, and nothing else.
//!
//! # Why a module TREE and not one file
//!
//! `fleet/sql.zig` reached its length cap and was carved into
//! `sql_lease_row.zig` and `sql_budget_drain.zig`, which are then re-exported
//! through `sql.zig` (`pub const INSERT_LEASE_WITH_EVENT = @import(…)`) purely
//! so RULE SQLMOD's "query text lives in one grepable module" survived the
//! split. That is a workaround for a gate, not a design. Rust has real modules,
//! so the split falls on DOMAIN instead of on line count, no file needs a
//! re-export to stay findable, and `grep -rn 'SELECT' src/sql/` still returns
//! everything.
//!
//! # Why collected at all, when `core_api` inlines its SQL
//!
//! `~/Projects/oss/core_api-develop` keeps SQL inline in `models/<entity>.rs`
//! and has no `sql.rs` anywhere — but its statements are one-line
//! stored-procedure calls (`SELECT * FROM insert_account_session_v2($1..$9)`)
//! whose logic lives in Postgres functions. That shape is unavailable here:
//! this milestone changes no schema, and the writable-CTEs port verbatim, so
//! `report`'s claim-and-settle is a ninety-line constant. More decisively, the
//! ONLY enforcement of verbatim-SQL parity is REVIEW reading these side by side
//! against the Zig originals — a read that cannot be done if the statements are
//! scattered through handler bodies.
//!
//! # The statements are byte-identical to their Zig originals
//!
//! Row-equivalence is the cutover invariant, so a statement is copied, not
//! re-derived. Where a `$n` order looks odd, it is odd in the original too and
//! is left alone; what changes is how it is BOUND — see [`runner::RegisterRow`]
//! for the shape high-arity statements take.

pub mod activity;
pub mod fleet;
pub mod gate;
pub mod grant;
pub mod lease;
pub mod memory;
pub mod provider;
pub mod renew;
pub mod report;
pub mod session;
pub mod vault;

/// The `fleet.runners.admin_state` value that permits the runner plane.
///
/// Imported rather than re-declared: `afd_state::sql` already owns this
/// spelling for the credential lookup that gates every runner-plane request,
/// and RULE UFS is explicit that a literal with a prior `const` declaration is
/// imported, never restated. Two spellings of "active" would mean a runner the
/// authenticator admits and this crate's writes consider dead.
pub use afd_state::sql::{
    ADMIN_STATE_ACTIVE, ADMIN_STATE_DRAINED, ADMIN_STATE_DRAINING, LAST_SEEN_NEVER,
    LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, LEASE_STATUS_REPORTED,
};
