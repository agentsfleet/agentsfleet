//! What a run may use, and how it reaches it.
//!
//! # Why this is not part of `afd_fleet`
//!
//! Three modules answer one question between them — a fleet DECLARES a
//! credential ([`secrets`]), a provider is DIALLED with what came out of the
//! vault ([`provider`]), and a broker MINTS a short-lived token for the child
//! that will use it ([`credential`]) — and none of them asks anything of a
//! lease.
//!
//! They looked entangled with the lease plane because one file was misfiled:
//! `credential/mint.rs` was 266 lines of `impl Plane`, an inherent impl on a
//! type this crate does not own. Moving it to the plane that owns the type
//! removed every edge from here to the lease, and what was left is this crate:
//! a five-level DAG with nothing above it.
//!
//! # The vault is `afd_vault`'s
//!
//! This crate opens no envelope of its own. It decides what a stored credential
//! MEANS — which declared name is a connector handle, which refresh token is
//! still the one that was posted, which provider a key belongs to — and asks
//! `afd_vault` for the bytes: `load_many` for a fleet's declared set,
//! `open_shared_on` for setup inspection under the caller's transaction,
//! `rewrite` for the refresh-token compare-and-swap, and `bag` for the
//! platform's own documents. The rows activation locks and the registry joins
//! are read here, and their envelopes go to `afd_vault` to be opened.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod error;

pub mod credential;
pub mod provider;
pub mod secrets;

pub use self::error::{Error, Result};
