//! Envelope encryption and message authentication for the `agentsfleetd` vault.
//!
//! Every credential the daemon stores is wrapped twice. A per-row Data
//! Encryption Key (DEK) encrypts the payload; the process Key Encryption Key
//! (KEK) encrypts that DEK; both operations bind the same associated data, so a
//! row cannot be replayed under a different workspace or a different name. That
//! layout is a STORED FORMAT: a row sealed by one release must open in the
//! next, so the two layers and their associated data are a contract with the
//! database rather than an implementation detail.
//!
//! # The oracles are external, on purpose
//!
//! Three of them: published NIST AES-256-GCM vectors pin the primitive, a
//! byte-exact assertion pins the associated-data format, and
//! `tests/envelope_conformance.rs` pins the envelope's behaviour case by case.
//! None is generated here. A fixture this crate produced would prove only that
//! it agrees with itself, which is the one thing never in doubt.
//!
//! # What the types guarantee
//!
//! Key material never appears in a public field, a `Debug` rendering, or a
//! `Display` rendering, and it is zeroed when dropped. That is enforced by
//! construction rather than by review: [`secret::Kek`] and [`secret::Dek`] wrap
//! private arrays, so there is no way to move the bytes out and leave an
//! un-zeroed copy behind. `test_secret_types_redact` holds the line.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Crate attribute rather than a workspace lint for the
// reason the workspace manifest gives: cargo lints apply to every target, and a
// dev-dependency legitimately goes unused by the library's own code.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod aad;
pub mod entropy;
pub mod envelope;
pub mod error;
pub mod mac;
pub mod secret;

/// Bytes in an AES-256 key.
pub const KEY_LEN: usize = 32;

/// Bytes in the AES-GCM nonce, matching `Aes256Gcm.nonce_length` on the Zig side.
pub const NONCE_LEN: usize = 12;

/// Bytes in the AES-GCM authentication tag, matching `Aes256Gcm.tag_length`.
pub const TAG_LEN: usize = 16;

/// The only KEK version any stored row carries.
///
/// `schema/039` adds a database CHECK that makes another value impossible, and
/// the associated data binds this number, so a row that somehow held a
/// different one fails its tag rather than opening. Version 1 is not supported:
/// nothing writes it and nothing reads it.
pub const KEK_VERSION: i32 = 2;
