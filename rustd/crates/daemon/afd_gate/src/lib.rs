//! What a fleet's run is permitted to do.
//!
//! # Why this is not part of `afd_fleet`
//!
//! [`policy`] assembles what a run may do — which tools it is offered, which
//! network reach, which credentials it holds and may mint, which repository a
//! write may touch — from the fleet's row and the grants a person approved for
//! it. The run plane calls DOWN into it once per run, and nothing here calls
//! back. [`gate`] holds the one statement a run's question is raised through.
//!
//! # A person's side is elsewhere
//!
//! `afd_approval` is the PERSON's: the queue a person browses, the one
//! decision Postgres admits, the grant an approved card leaves behind, and the
//! answer turn that resumes a waiting run. This crate reads the approved
//! grants and nothing else of that table.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod error;

pub mod gate;
pub mod policy;

pub use self::error::{Error, Result};
