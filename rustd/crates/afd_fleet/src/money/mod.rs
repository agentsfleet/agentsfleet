//! What a lease costs, what a tenant has, and what a fleet has already spent.
//!
//! # Why this is a module and not three
//!
//! The Zig spreads this across `state/tenant_billing.zig` (the wallet and the
//! rate constants), `state/tenant_billing_rates.zig` (the arithmetic),
//! `fleet/budget.zig` (the ceilings) and `fleet_runtime/metering.zig` (the
//! debits) — four files in three directories, each importing the ones above
//! it. The split is real but it is a split by LAYER, and the layers all answer
//! one question: how much, and is there enough.
//!
//! Collected here, the dependency runs one way and is visible at a glance:
//! [`nanos`] is pure arithmetic over a unit, [`window`] is pure arithmetic over
//! time, and everything with a connection sits on top of both. Nothing in the
//! two pure modules can reach a datastore, which is what makes the money
//! arithmetic testable without one — and it is the property the Zig has to
//! assert with a comment because its `tenant_billing.zig` and its
//! `tenant_billing_rates.zig` can both see a `*pg.Conn`.
//!
//! # The separation this module is built around
//!
//! A gate asks two different questions and the Zig answers them in one place:
//! *what is the verdict* and *what do we do when we cannot reach the datastore
//! to find out*. Those are separated here — a read answers a value or an
//! [`Error`](crate::Error), and the fail-open or fail-closed POSTURE belongs to
//! the caller in [`crate::lease::admit`], declared once per gate beside its
//! name rather than decided at each `catch`.
//!
//! `budget.zig` reached the same conclusion on its own and says so: splitting
//! "we read nothing" into distinct causes is what lets the decision be a pure
//! function instead of something buried beside a connection. This module takes
//! that as the rule rather than the exception.

pub mod budget;
pub mod charge;
pub mod nanos;
pub mod rates;
pub mod store;
pub mod wallet;
pub mod window;

pub use self::budget::{Spend, Verdict};
pub use self::charge::Charged;
pub use self::nanos::{
    ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, NANOS_PER_USD, Nanos, RECEIVE_NANOS,
    RUN_NANOS_PER_SEC, SliceRates, slice_charge,
};
pub use self::rates::Posture;
pub use self::store::Accounts;
pub use self::wallet::Wallet;
pub use self::window::Windows;
