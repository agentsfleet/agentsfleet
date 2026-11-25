//! What a run is permitted to do, assembled for one run.
//!
//! The run's answer carries an `ExecutionPolicy`: where the run may reach,
//! which secrets it holds, which it may mint, which tools it is offered and
//! which provider it bills against. This module is where a fleet's row and the
//! grants a person approved for it become that policy.
//!
//! [`approved`] reads the grants; [`catalog`], [`egress`] and [`context`] are
//! the parts that have nothing to do with datastores — the tool list, an
//! approved GitHub scope turned into an allow-list of exact HTTP requests, and
//! the per-run context budget — so they live apart and are proven as pure
//! functions over values a caller read somewhere else.

pub mod approved;
pub mod build;
pub mod catalog;
pub mod context;
pub mod egress;
pub mod repair;
mod shape;

pub use self::shape::Network;

#[cfg(test)]
mod fixture;
