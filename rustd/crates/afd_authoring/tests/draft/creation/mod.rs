//! Retained setup, atomic activation and cancellation use the live stores.

#![expect(
    clippy::indexing_slicing,
    reason = "a proof indexes the rows it has just asserted the count of"
)]

mod activation;
mod admission_faults;
mod cancellation;
mod confirmation;
mod contention;
mod deletion_matrix;
mod fixture;
mod funnel;
mod grants;
mod identity;
mod improvement;
mod interleaving;
mod race;
mod refusals;
mod retries;
mod rollback;
mod stages;
mod stopped;
mod stored;
