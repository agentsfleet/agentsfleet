//! The unit every ledger row is denominated in, and the one conversion into it.
//!
//! # Why nanos, and why an integer
//!
//! `billing.usage_ledger.credit_deducted_nanos` is `BIGINT`, and every drain in
//! this product is a whole number of nanos. One US dollar is
//! [`NANOS_PER_USD`]; the rate constants below are the Zig ledger's, imported
//! rather than re-derived, because `audits/cross-tier-rates.sh` pins their
//! Zig spellings across four files and a second set of numbers here would be a
//! second source of truth for what a second of runtime costs.
//!
//! # Why not a decimal crate
//!
//! Asked and declined, deliberately. A fixed-point decimal is the textbook
//! answer for money and it is the wrong answer HERE, for one reason: the only
//! thing this arithmetic is graded on is agreeing with the daemon it replaces.
//!
//! An authored ceiling arrives as `f64` — `Dollars` holds one, because the
//! authoring document spells `daily_dollars: 5.0` and the Zig parser reads a
//! float. The Zig then computes `@round(dollars * 1e9)`. Routing that through a
//! decimal would round a different way at the boundary and produce a ceiling
//! one nano from the Zig's on some inputs. That is more correct in the abstract
//! and a DIVERGENCE in the row-equivalence this milestone exists to prove
//! (Invariant 5).
//!
//! The place to argue for decimals is the schema — make the authored ceiling a
//! decimal string and the float disappears at the source. That is a schema
//! change, this milestone changes no schema, and doing half of it here would
//! leave the product with two roundings instead of one.

use afd_fleet_runtime::config::Dollars;

/// A quantity of credit, in nanos.
///
/// A newtype rather than a bare `i64` because this crate also passes token
/// counts, millisecond spans and fencing tokens as `i64`, and every one of them
/// is assignable to every other. The wrapper is what makes a run fee that got
/// bound into an elapsed-milliseconds parameter fail to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Nanos(i64);

/// Nanos in one US dollar.
///
/// `tenant_billing.zig`'s `NANOS_PER_USD`, which `ui/packages/app/lib/types.ts`
/// and `cli/src/constants/billing.js` also carry — three spellings of one
/// constant, and this is the fourth. It is restated rather than imported
/// because there is nothing in Rust yet to import it FROM; the cross-tier audit
/// is what keeps the four honest.
pub const NANOS_PER_USD: i64 = 1_000_000_000;

/// What receiving one event costs, under either posture.
///
/// Zero, and that is not a placeholder: `computeReceiveCharge` discards its
/// posture argument and returns `EVENT_NANOS`. The debit still fires, and still
/// writes its ledger row, because the ROW is what the budget drain and the
/// charges endpoint read — a charge of zero that is recorded is a different
/// thing from a charge that never happened, and only one of them can later be
/// priced without a migration.
pub const RECEIVE_NANOS: Nanos = Nanos(0);

/// What one second of active runtime costs, under either posture.
///
/// `tenant_billing.zig`'s `RUN_NANOS_PER_SEC` — $0.0001/sec, about $0.36/hour.
pub const RUN_NANOS_PER_SEC: i64 = 100_000;

/// The input-token floor the issue-time estimate is sized against.
///
/// The runner does not know its real token counts at lease time, so the gate
/// prices a deliberately small floor. `tenant_billing.zig`'s
/// `ESTIMATE_FLOOR_INPUT_TOKENS`.
pub const ESTIMATE_FLOOR_INPUT_TOKENS: i64 = 100;

/// The output-token floor the issue-time estimate is sized against.
pub const ESTIMATE_FLOOR_OUTPUT_TOKENS: i64 = 100;

/// [`NANOS_PER_USD`] as a float, for the one conversion that needs it.
///
/// A separate literal rather than `NANOS_PER_USD as f64`, and not to silence a
/// lint: an `i64`-to-`f64` cast is lossy in general — `f64` carries 53 bits of
/// mantissa against 64 bits of integer — so writing the cast invites a reader
/// to wonder whether the scale factor is exact. It is: 10^9 is well under
/// 2^53, so this literal and the integer constant name the same number with no
/// rounding between them. `nanos_per_usd_is_exact_in_both_representations`
/// keeps the two from drifting apart.
const NANOS_PER_USD_F64: f64 = 1e9;

/// Tokens in the unit a catalogue rate is quoted per.
const TOKENS_PER_MTOK: i64 = 1_000_000;

/// Milliseconds in the unit [`RUN_NANOS_PER_SEC`] is quoted per.
const MS_PER_SEC: i64 = 1_000;

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

    /// Whether this is a charge at all.
    ///
    /// The receive charge is zero today, and a caller that skips the ledger
    /// write on that basis would break the budget drain — so this exists for
    /// LOGGING and nothing else, and no gate branches on it.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// A declared ceiling, in nanos.
    ///
    /// Total, with no guard clause, and the absent guard is the point.
    /// `dollarsToNanos` opens with `if (!isFinite(dollars) or dollars <= 0)
    /// return 0` because it takes a bare `f64` and a caller may hand it
    /// anything. [`Dollars`] cannot be built from a value that is not finite,
    /// not positive, or above its cap — `Dollars::parse` refuses all three — so
    /// the branch here would be unreachable code asserting a property the
    /// argument already carries.
    ///
    /// The saturating branch is absent for a different reason: Rust's
    /// float-to-integer cast saturates by language rule, so the overflow the
    /// Zig guards with `if (scaled >= maxInt(i64))` cannot wrap here even if a
    /// future cap change made it reachable. Rounding is to nearest, matching
    /// `@round`, so a ceiling of one nano is one nano rather than zero.
    #[must_use]
    pub fn from_dollars(dollars: Dollars) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the float-to-integer cast saturates by language rule, \
                      which is exactly what the Zig spells as its \
                      `scaled >= maxInt(i64)` branch; `Dollars` bounds the \
                      input four orders of magnitude below that point anyway"
        )]
        Self((dollars.dollars() * NANOS_PER_USD_F64).round() as i64)
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
    /// `daily_dollars` runs no further. `covers` in `budget.zig` spells the
    /// same comparison as `spend >= dollarsToNanos(cap)`.
    #[must_use]
    pub const fn has_reached(self, ceiling: Self) -> bool {
        self.0 >= ceiling.0
    }
}

/// The four per-unit rates one metered slice is charged at.
///
/// Resolved once and applied in two places — here, and as bind parameters to
/// the renewal CTE — so the SQL charges what this charges by construction
/// rather than by a rate table copied into a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceRates {
    /// Charged per second of active runtime, under both postures.
    pub run_nanos_per_sec: i64,
    /// Charged per million input tokens. Zero under self-managed.
    pub input_nanos_per_mtok: i64,
    /// Charged per million cached input tokens. Zero under self-managed.
    pub cached_input_nanos_per_mtok: i64,
    /// Charged per million output tokens. Zero under self-managed.
    pub output_nanos_per_mtok: i64,
}

/// What a slice costs at these rates.
///
/// Pure, total, and the reference the SQL is pinned against. Every division
/// truncates toward zero, which is what Postgres `bigint` division does for
/// non-negative operands and what Zig's `@divTrunc` does — the three agree only
/// because all three truncate, so a change to rounding here is a change to the
/// money in three places.
#[must_use]
pub const fn slice_charge(
    rates: SliceRates,
    elapsed_ms: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> Nanos {
    let run = elapsed_ms * rates.run_nanos_per_sec / MS_PER_SEC;
    let input = rates.input_nanos_per_mtok * input_tokens / TOKENS_PER_MTOK;
    let cached = rates.cached_input_nanos_per_mtok * cached_input_tokens / TOKENS_PER_MTOK;
    let output = rates.output_nanos_per_mtok * output_tokens / TOKENS_PER_MTOK;
    Nanos(run + input + cached + output)
}

#[cfg(test)]
mod tests {
    use super::{
        ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, NANOS_PER_USD,
        NANOS_PER_USD_F64, Nanos, RUN_NANOS_PER_SEC, SliceRates, slice_charge,
    };

    /// The rates a self-managed tenant meters at: runtime only.
    const SELF_MANAGED: SliceRates = SliceRates {
        run_nanos_per_sec: RUN_NANOS_PER_SEC,
        input_nanos_per_mtok: 0,
        cached_input_nanos_per_mtok: 0,
        output_nanos_per_mtok: 0,
    };

    #[test]
    fn a_slice_under_self_managed_is_the_run_fee_and_nothing_else() {
        // Twenty seconds of runtime, and a million tokens of every kind: the
        // tokens land on the tenant's own provider bill, never here.
        // pin test: literal is the contract
        let charged = slice_charge(SELF_MANAGED, 20_000, 1_000_000, 1_000_000, 1_000_000);
        assert_eq!(charged, Nanos::from_i64(20 * RUN_NANOS_PER_SEC));
    }

    #[test]
    fn the_run_fee_keeps_millisecond_precision_and_truncates() {
        // 1_500ms is a second and a half; truncating division floors it to the
        // nano rather than rounding up, which is what the renewal CTE does.
        assert_eq!(
            slice_charge(SELF_MANAGED, 1_500, 0, 0, 0),
            Nanos::from_i64(150_000)
        );
        // At lease issue no time has elapsed, so the estimate carries no run
        // fee at all — only the token floor, and only under platform posture.
        assert_eq!(slice_charge(SELF_MANAGED, 0, 0, 0, 0), Nanos::ZERO);
    }

    #[test]
    fn token_tiers_are_priced_per_million_and_truncate_toward_zero() {
        let platform = SliceRates {
            run_nanos_per_sec: 0,
            input_nanos_per_mtok: 3_000_000,
            cached_input_nanos_per_mtok: 300_000,
            output_nanos_per_mtok: 15_000_000,
        };
        // The estimate floor: 100 in, 100 out, nothing cached. Each tier
        // truncates independently, exactly as four separate `@divTrunc` calls
        // do — summing first and dividing once would answer differently.
        let charged = slice_charge(
            platform,
            0,
            ESTIMATE_FLOOR_INPUT_TOKENS,
            0,
            ESTIMATE_FLOOR_OUTPUT_TOKENS,
        );
        assert_eq!(charged, Nanos::from_i64(300 + 1_500));
    }

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
        const { assert!(NANOS_PER_USD_F64 == 1_000_000_000.0_f64) }

        // And a five-dollar daily ceiling is five billion nanos.
        assert_eq!(NANOS_PER_USD * 5, 5_000_000_000);
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
