//! Durable acceptance and capacity use the production ledger statements.

/// What a receipt case returns: any failed step ends it and fails the test.
pub(crate) type Fallible<T = ()> = Result<T, Box<dyn std::error::Error>>;

mod budgets;
mod confirmation;
mod failures;
mod lane;
mod ledger;
mod locking;
mod lookups;
mod queue;
mod staging;
