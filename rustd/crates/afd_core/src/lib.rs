//! Domain primitives with no input/output, shared by every `agentsfleetd` crate.
//!
//! This crate is the bottom of the dependency graph. It holds the values the
//! rest of the daemon agrees on — canonical entity identifiers, the wire error
//! codes, and the bounded numbers policy is expressed in — and nothing else. It
//! opens no socket, reads no file, spawns no thread, and pulls in no
//! asynchronous runtime; `test_core_dependency_freeze` asserts that against the
//! real dependency graph rather than trusting this paragraph.
//!
//! # Why the primitives live here rather than beside their users
//!
//! Each type in this crate encodes an invariant that more than one layer has to
//! agree on. [`id::Uuid7`] fixes one spelling for an entity identifier, so a
//! wire payload, a database row and a cache key can never disagree about
//! whether two identifiers are the same. [`error_code::ErrorCode`] fixes the
//! shape of the codes a client matches on. [`limits`] fixes the clamps that
//! must be applied identically at the assignment surface and at the host.
//!
//! Every one of them is a newtype with a fallible constructor rather than a
//! type alias: the invariant is checked once, at the boundary, and every later
//! use can rely on it.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit, and an unused-but-linked runtime is exactly how this
// crate would breach Invariant 2. Crate-attribute rather than workspace lint:
// see the note in the workspace Cargo.toml. Gated on `not(test)` because the
// test build links dev-dependencies into this same target, where a
// test-only crate legitimately goes unused by the library's own code — the
// claim being made is about the SHIPPED library's dependency graph.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod clock;
pub mod env;
pub mod error;
pub mod error_code;
pub mod id;
pub mod limits;
pub mod problem;
pub mod timing;
