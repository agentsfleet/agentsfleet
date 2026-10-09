//! What a lease costs, what a tenant has, and what a fleet has already spent.
//!
//! # Why this is a module and not three
//!
//! The wallet and rate constants, the arithmetic, the ceilings and the debits
//! are four layers, each depending on the ones above it. The split is real but
//! it is a split by LAYER, and the layers all answer one question: how much,
//! and is there enough.
//!
//! Collected here, the dependency runs one way and is visible at a glance:
//! [`nanos`] is pure arithmetic over a unit, [`window`] is pure arithmetic over
//! time, and everything with a connection sits on top of both. Nothing in the
//! two pure modules can reach a datastore, which is what makes the money
//! arithmetic testable without one — a property the module boundary enforces
//! rather than a comment asserting it.
//!
//! # The separation this module is built around
//!
//! A gate asks two different questions: *what is the verdict* and *what do we
//! do when we cannot reach the datastore to find out*. Those are separated
//! here — a read answers a value or an [`Error`](crate::Error), and the
//! fail-open or fail-closed POSTURE belongs to the caller in
//! `afd_fleet::lease::admit`, declared once per gate beside its name rather
//! than decided at each error arm.
//!
//! Splitting "we read nothing" into distinct causes is what lets the decision
//! be a pure function instead of something buried beside a connection. This
//! module takes that as the rule rather than the exception.

pub mod error;

pub mod budget;
pub mod charge;
pub mod meter;
pub mod nanos;
pub mod rates;
pub mod sql;
// The TENANT's read side of the same schema: a wallet snapshot and the paged
// charge ledger a person reads in the console. It sat in `afd_tenant` while
// everything that WRITES those rows sat here — so a future billing binary
// would have had to link the api-key and login plane to serve a balance.
pub mod store;
pub mod tenant;
mod tenant_sql;
pub mod wallet;
pub mod window;

pub use self::budget::{Spend, Verdict};
pub use self::charge::Charged;
pub use self::error::{Error, Result};
pub use self::meter::{Cumulative, Meter};
pub use self::nanos::{
    ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, NANOS_PER_USD, Nanos, RECEIVE_NANOS,
    RUN_NANOS_PER_SEC, SliceRates, slice_charge,
};
pub use self::rates::Posture;
pub use self::store::Accounts;
pub use self::wallet::Wallet;
pub use self::window::Windows;
