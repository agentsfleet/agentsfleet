//! What a run is permitted to do, assembled for one lease.
//!
//! The lease's answer carries an `ExecutionPolicy`: where the run may reach,
//! which secrets it holds, which it may mint, and which provider it bills
//! against. This module is where a fleet's stored config becomes that policy.
//!
//! [`egress`] is the half that has nothing to do with datastores — a repository
//! binding turned into an allow-list of exact HTTP requests — so it lives
//! apart and is proven as a pure function.

pub mod egress;
