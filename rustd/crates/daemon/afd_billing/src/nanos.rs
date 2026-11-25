//! The unit every ledger row is denominated in, and the one conversion into it.
//!
//! # Why nanos, and why an integer
//!
//! `billing.usage_lines.charged_nanos` is `BIGINT`, and every drain in
//! this product is a whole number of nanos. One US dollar is
//! [`NANOS_PER_USD`], which [`af_core::money`] declares because an account
//! balance is denominated in it too. The rates below are this crate's alone:
//! the daemon is what charges a run, and a client renders the charge it was
//! served rather than recomputing it, so a bump lands here and nowhere else.
//!
/// A quantity of credit, in nanos.
///
/// A newtype rather than a bare `i64` because this crate also passes token
/// counts, millisecond spans and fencing tokens as `i64`, and every one of them
/// is assignable to every other. The wrapper is what makes a run fee that got
/// bound into an elapsed-milliseconds parameter fail to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Nanos(i64);

/// What one second of active runtime costs, under either posture.
///
/// $0.0001/sec, about $0.36/hour. Declared once: the daemon is what charges a
/// run, and no client recomputes a fee it is served.
pub const RUN_NANOS_PER_SEC: i64 = 100_000;

/// The input-token floor the issue-time estimate is sized against.
///
/// The runner does not know its real token counts at run time, so the gate
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
/// `pub` because the run metering statement converts its elapsed interval
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

    /// This quantity plus `other`, refusing to wrap.
    ///
    /// Saturating rather than checked: the sum of two ledger amounts has no
    /// meaningful failure answer at a call site, and an `i64` of nanos is nine
    /// billion dollars — a total that reached it is already a fault somewhere
    /// upstream, and clamping keeps it visible in the row instead of wrapping
    /// it negative where a funds check would read it as credit.
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
}

#[cfg(test)]
mod tests {
    use super::Nanos;

    #[test]
    fn a_balance_covers_a_cost_it_equals_and_not_one_past_it() {
        let amount = Nanos::from_i64(1_000);
        // The credit gate admits a run it can exactly afford, and no more.
        assert!(amount.covers(amount));
        assert!(!amount.covers(Nanos::from_i64(1_001)));
    }

    #[test]
    fn adding_ledger_amounts_clamps_rather_than_wrapping_negative() {
        // A wrapped total would read as CREDIT to the funds check, which is the
        // one failure mode worth spending a branch on.
        let huge = Nanos::from_i64(i64::MAX);
        assert_eq!(huge.saturating_add(Nanos::from_i64(1)), huge);
    }
}
