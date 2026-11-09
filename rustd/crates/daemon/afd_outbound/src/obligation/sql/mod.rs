//! The statements an owed answer moves through, from owed to delivered.
//!
//! Every write after the owe matches the holder's token in `receipt`, so a
//! holder whose claim lapsed cannot stamp, count, release or abandon a row
//! another holder has since taken, and two holders never both post one
//! answer. Deadlines are computed from
//! `clock_timestamp()` plus a bound interval; the claim compares them with
//! `statement_timestamp()`.

/// Owes an answer, free to claim at once. `$1` id · `$2` fleet · `$3`
/// workspace · `$4` provider · `$5` event · `$6` answer · `$7` destination ·
/// `$8` the owing report's token. A repeat of one event owes nothing.
pub(crate) const OWE_DELIVERY: &str = include_str!("owe_delivery.sql");

/// Locks up to `$1` lanes whose head is claimable, for the transaction,
/// skipping lanes another claim holds; answers each lane's provider and
/// workspace. Runs before [`CLAIM_HEADS`], in the same transaction.
pub(crate) const LOCK_LANES: &str = include_str!("lock_lanes.sql");

/// Claims up to `$1` lane heads among the lanes [`LOCK_LANES`] locked: open,
/// due, unheld, with no earlier open row in their (provider, workspace) lane,
/// in `sequence` order. `$2` the claim's token · `$3` the claim's life ·
/// `$4` the locked lanes' providers · `$5` their workspaces, pairwise.
pub(crate) const CLAIM_HEADS: &str = include_str!("claim_heads.sql");

/// Extends a live claim. `$1` id · `$2` token · `$3` the claim's life.
pub(crate) const RENEW_CLAIM: &str = include_str!("renew_claim.sql");

/// Gives a claim back after a retryable failure, due again after `$3`.
/// `$1` id · `$2` token.
pub(crate) const RELEASE_CLAIM: &str = include_str!("release_claim.sql");

/// Records delivery. `$1` id · `$2` token; zero rows means the claim lapsed.
pub(crate) const STAMP_DELIVERED: &str = include_str!("stamp_delivered.sql");

/// Counts one delivery cycle, extends the claim by `$3`, and answers the new
/// count; no row means another holder has it. `$1` id · `$2` token.
pub(crate) const COUNT_ATTEMPT: &str = include_str!("count_attempt.sql");

/// Gives up on an answer. `$1` id · `$2` token · `$3` reason.
pub(crate) const ABANDON: &str = include_str!("abandon.sql");
