//! What a run is permitted to do, assembled for one lease.
//!
//! The lease's answer carries an `ExecutionPolicy`: where the run may reach,
//! which secrets it holds, which it may mint, and which provider it bills
//! against. This module is where a fleet's stored config becomes that policy.
//!
//! [`egress`] and [`context`] are the halves that have nothing to do with
//! datastores — a repository binding turned into an allow-list of exact HTTP
//! requests, and the per-lease context budget — so they live apart and are
//! proven as pure functions.

pub mod context;
pub mod egress;
