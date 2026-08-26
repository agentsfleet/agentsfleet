//! What a run is permitted to do, assembled for one lease.
//!
//! The lease's answer carries an `ExecutionPolicy`: where the run may reach,
//! which secrets it holds, which it may mint, and which provider it bills
//! against. This module is where a fleet's stored config becomes that policy.
//!
//! [`egress`], [`context`] and [`grants`] are the parts that have nothing to do
//! with datastores — a repository binding turned into an allow-list of exact
//! HTTP requests, the per-lease context budget, and the set of integrations a
//! workspace stands behind — so they live apart and are proven as pure
//! functions over values a caller read somewhere else.

pub mod build;
pub mod context;
pub mod egress;
pub mod grants;
mod shape;

#[cfg(test)]
mod fixture;
