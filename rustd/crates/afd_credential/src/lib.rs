//! What a run may use, and how it reaches it.
//!
//! # Why this is not part of `afd_fleet`
//!
//! Four modules answer one question between them — a fleet DECLARES a
//! credential ([`secrets`]), the vault OPENS it ([`vault`]), a provider is
//! DIALLED with what came out ([`provider`]), and a broker MINTS a short-lived
//! token for the child that will use it ([`credential`]) — and none of them
//! asks anything of a lease.
//!
//! They looked entangled with the lease plane because one file was misfiled:
//! `credential/mint.rs` was 266 lines of `impl Plane`, an inherent impl on a
//! type this crate does not own. Moving it to the plane that owns the type
//! removed every edge from here to the lease, and what was left is this crate:
//! a five-level DAG with nothing above it.
//!
//! # This is the RUNNER's side of a secret
//!
//! `afd_vault` is the OPERATOR's — the workspace surface that seals a write,
//! lists without decrypting, and holds a reference lock over a delete. This
//! crate opens a credential a fleet declared, refuses to degrade a row it
//! cannot read, and never lists. Two failure policies over one table, and
//! folding them together would mean one of the two losing.

mod error;

pub mod credential;
pub mod provider;
pub mod secrets;
pub mod vault;

pub use self::error::{Error, Result};
