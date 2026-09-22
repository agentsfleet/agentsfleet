//! The unit every credit column is denominated in.
//!
//! One US dollar is [`NANOS_PER_USD`] nanos, and that is a property of the wire
//! format rather than a price: `billing.usage_ledger.credit_deducted_nanos` and
//! `balance_nanos` are `BIGINT` columns of nanos, so nothing can read one
//! without this factor. Rates are a different thing and live with the code that
//! charges them — [`afd_billing`] for what runtime costs, `afd_tenant::signup`
//! for the starter grant.
//!
//! # Why here and not in the billing crate
//!
//! Two crates need the factor and neither should depend on the other for it.
//! `afd_billing` prices a lease; `afd_tenant` opens an account with a balance.
//! Making the second depend on the first would pull a datastore, a credential
//! store and the fleet runtime behind one integer. Both already depend on this
//! crate, so declaring it here costs no edge in the dependency graph — the same
//! reason [`crate::timing`] holds the lease clock instead of the crate that
//! issues leases.

/// Nanos in one US dollar.
///
/// Every client that renders a balance divides by this, which is why it is a
/// declaration and not an inline divisor: `nanos / 1_000_000_000` names the
/// arithmetic but not what the divisor IS.
pub const NANOS_PER_USD: i64 = 1_000_000_000;

/// The largest integer a double can hold without losing a unit.
///
/// JavaScript has no integer type, so every balance this product serves is read
/// by a client that holds it as a double. A nanos value above this arrives
/// rounded, and a rounded balance is a wrong one.
///
/// This bound is headroom, not a cap. `balance_nanos` is an unbounded `BIGINT`
/// and nothing refuses a top-up that crosses the line, so a wallet above
/// roughly nine million USD would lose precision at the client. Enforcing a
/// maximum is a billing change and is not made here — stated so the constant
/// is not read as a guarantee it does not carry.
const EXACT_IN_F64: i64 = 1_i64 << 53;

const _: () = assert!(
    NANOS_PER_USD < EXACT_IN_F64,
    "a balance must survive the trip through a JavaScript number"
);

#[cfg(test)]
mod tests {
    use super::{EXACT_IN_F64, NANOS_PER_USD};

    /// The factor is the wire format, so the literal is what is being asserted.
    #[test]
    fn test_one_dollar_is_a_billion_nanos() {
        // pin test: literal is the contract
        assert_eq!(NANOS_PER_USD, 1_000_000_000);
    }

    /// The headroom the assertion above buys, stated as a number a reader can
    /// weigh: a balance below roughly nine million USD stays exact in a double.
    /// Nothing holds a balance below it; see `EXACT_IN_F64`.
    #[test]
    fn test_a_balance_stays_exact_well_past_any_real_one() {
        const { assert!(EXACT_IN_F64 / NANOS_PER_USD > 9_000_000) };
    }
}
