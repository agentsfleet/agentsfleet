//! Retained setup, atomic activation and cancellation use the live stores.

#![expect(
    clippy::indexing_slicing,
    reason = "a proof indexes the rows it has just asserted the count of"
)]

mod activation;
mod cancellation;
mod contention;
mod fixture;
mod funnel;
mod grants;
mod identity;
mod race;
mod retries;
mod stages;
