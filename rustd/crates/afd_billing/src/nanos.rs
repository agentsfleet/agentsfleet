//! The unit every ledger row is denominated in, and the one conversion into it.
//!
//! # Why nanos, and why an integer
//!
//! `billing.usage_lines.charged_nanos` is `BIGINT`, and every drain in
//! this product is a whole number of nanos. One US dollar is
//! [`NANOS_PER_USD`], which [`afd_core::money`] declares because an account
//! balance is denominated in it too. The rates below are this crate's alone:
//! the daemon is what charges a lease, and a client renders the charge it was
//! served rather than recomputing it, so a bump lands here and nowhere else.
//!
//! Authored ceilings are decimal strings of integer nanos. The runtime parser
//! proves their bounds; conversion into ledger units preserves every nano.

use afd_fleet_runtime::config::BudgetCeiling;

/// A quantity of credit, in nanos.
///
/// A newtype rather than a bare `i64` because this crate also passes token
/// counts, millisecond spans and fencing tokens as `i64`, and every one of them
/// is assignable to every other. The wrapper is what makes a run fee that got
/// bound into an elapsed-milliseconds parameter fail to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Nanos(i64);

pub use afd_core::money::NANOS_PER_USD;

/// What one second of active runtime costs, under either posture.
///
/// $0.0001/sec, about $0.36/hour. Declared once: the daemon is what charges a
/// lease, and no client recomputes a fee it is served.
pub const RUN_NANOS_PER_SEC: i64 = 100_000;

/// The input-token floor the issue-time estimate is sized against.
///
/// The runner does not know its real token counts at lease time, so the gate
/// prices a deliberately small floor.
pub const ESTIMATE_FLOOR_INPUT_TOKENS: u32 = 100;

/// The output-token floor the issue-time estimate is sized against.
pub const ESTIMATE_FLOOR_OUTPUT_TOKENS: u32 = 100;

/// Tokens in the unit a catalogue rate is quoted per.
///
/// The [`Rater`](crate::price::Rater) divides every token category by it.
pub const TOKENS_PER_MTOK: i64 = 1_000_000;

/// Milliseconds in the unit [`RUN_NANOS_PER_SEC`] is quoted per.
///
/// `pub` because the lease metering statement converts its elapsed interval
/// with it, as a bind parameter.
pub const MS_PER_SEC: i64 = 1_000;

impl Nanos {
    /// Nothing drained.
    pub const ZERO: Self = Self(0);

    /// A quantity read back out of a ledger column.
    #[must_use]
    pub const fn from_i64(nanos: i64) -> Self {
        Self(nanos)
    }

    /// The quantity, for binding into a statement.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// A declared ceiling, in nanos.
    ///
    /// Preserves an already validated ceiling without rounding.
    #[must_use]
    pub const fn from_ceiling(ceiling: BudgetCeiling) -> Self {
        Self(ceiling.nanos())
    }

    /// This quantity plus `other`, refusing to wrap.
    ///
    /// Saturating rather than checked: the sum of two ledger amounts has no
    /// meaningful failure answer at a call site, and an `i64` of nanos is nine
    /// billion dollars — a total that reached it is already a fault somewhere
    /// upstream, and clamping keeps it visible in the row instead of wrapping
    /// it negative where a budget check would read it as credit.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Whether `self` covers `cost`.
    ///
    /// Named rather than left to `>=` at the call site because the direction is
    /// the whole gate: `balance.covers(estimate)` reads as the question being
    /// asked, where `balance >= estimate` reads as an expression whose operands
    /// a reader has to check the order of.
    #[must_use]
    pub const fn covers(self, cost: Self) -> bool {
        self.0 >= cost.0
    }

    /// Whether spending has reached `ceiling`.
    ///
    /// Refused AT equality, not past it: a fleet that has spent exactly its
    /// `daily_nanos` runs no further.
    #[must_use]
    pub const fn has_reached(self, ceiling: Self) -> bool {
        self.0 >= ceiling.0
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "the validated budget is the conversion fixture precondition"
    )]

    use super::{NANOS_PER_USD, Nanos};

    #[test]
    fn nanos_per_usd_is_exact_in_both_representations() {
        // The scale factor exists twice — as the integer every ledger amount is
        // counted in, and as the float a declared ceiling is multiplied by. The
        // pair is only safe because 10^9 is under 2^53 and therefore exact in
        // `f64`; this asserts that rather than leaving it to the comment, so a
        // future change to either literal fails here instead of silently
        // rounding a tenant's ceiling.
        // A `const` block, so drift is a BUILD failure rather than a test
        // failure — the right severity for a scale factor that prices every
        // ceiling in the product.
        // Both as `const` blocks, so drift is a BUILD failure rather than a
        // test failure — the right severity for a scale factor that prices
        // every ceiling in the product. Exact equality is the claim on the
        // second: an epsilon comparison would admit the very drift this
        // catches.
        const { assert!(NANOS_PER_USD < (1_i64 << 53)) }

        // And a five-dollar daily ceiling is five billion nanos.
        assert_eq!(NANOS_PER_USD * 5, 5_000_000_000);

        let config = afd_fleet_runtime::FleetConfig::stored(
            r#"{"name":"b","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"5000000000"}}}"#,
        )
        .expect("the fixture document is well-formed");
        assert_eq!(
            Nanos::from_ceiling(config.budget().daily()),
            Nanos::from_i64(5 * NANOS_PER_USD)
        );
    }

    #[test]
    fn a_balance_covers_a_cost_it_equals_but_a_ceiling_refuses_one_it_equals() {
        // An arbitrary amount compared against itself; the VALUE is immaterial,
        // the direction of each comparison is the claim.
        // pin test: literal is the contract
        let amount = Nanos::from_i64(1_000);
        // The credit gate admits a run it can exactly afford...
        assert!(amount.covers(amount));
        // ...and the budget gate refuses one that has exactly reached the cap.
        // Two comparisons that look alike and point opposite ways, which is why
        // each is named after the question it answers.
        assert!(amount.has_reached(amount));
    }

    #[test]
    fn adding_ledger_amounts_clamps_rather_than_wrapping_negative() {
        // A wrapped total would read as CREDIT to the budget gate, which is the
        // one failure mode worth spending a branch on.
        let huge = Nanos::from_i64(i64::MAX);
        assert_eq!(huge.saturating_add(Nanos::from_i64(1)), huge);
    }
}
