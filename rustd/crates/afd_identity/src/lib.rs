//! The identity provider's side of authentication.
//!
//! `afd_auth` owns the DECISION — which credential class a value belongs to,
//! what proves it, and what a refusal says. This crate owns the two pieces of
//! that decision which reach the network, behind the seams `afd_auth` declares:
//! [`jwks::verifier::JwksVerifier`] implements `TokenVerifier`, and
//! [`capability::ProviderCapabilities`] implements `CapabilitySource`.
//!
//! # Why this is a separate crate
//!
//! The Zig daemon holds a portability wall — `src/agentsfleetd/auth/` must not
//! import `src/db/`, `src/http/`, or any business module — and enforces it with
//! a grep in `make test-auth`. Splitting the I/O out here makes the same rule a
//! fact about the dependency graph: `afd_auth` does not list `reqwest`, `moka`
//! or `ring`, so it cannot name them, and rustc checks that on every build
//! rather than a script checking it on demand.
//!
//! It also keeps `afd_auth` provable offline. Every routing, liveness and
//! refusal branch over there runs with no runtime, no socket and no clock — and
//! that stays true only while the network lives somewhere else.
//!
//! The three Postgres `CredentialDirectory` implementations are NOT here. They
//! belong with the host, which is where the Zig daemon keeps them
//! (`cmd/serve_runner_lookup.zig`, `cmd/cli_credential_lookup.zig`), and for
//! this port that is §5.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target, where a test-only crate legitimately
// goes unused by the library's own code.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
// Every duplicate in this crate's graph is inside its dependencies', not ours:
// `reqwest` and `sqlx` pull an older `base64` line and the RustCrypto 0.10 one
// through `ring`/`rustls`. This workspace's own pins are the current line, so
// there is nothing here to unify. `expect`, so it fails the build once that
// stops being true rather than sitting here forever.
#![expect(
    clippy::multiple_crate_versions,
    reason = "reqwest and rustls pin transitive versions this workspace does not choose"
)]

pub mod capability;
pub mod error;
pub mod jwks;
mod jwt;
pub mod metadata;
pub mod provider;

pub use crate::capability::ProviderCapabilities;
pub use crate::error::{ClaimUnavailable, Error, MetadataUnwritten, Result};
pub use crate::jwks::http::{HttpKeySet, jwks_url};
pub use crate::jwks::key_set::{JwkKeySet, SigningKey};
pub use crate::jwks::source::{KeySetSource, StaticKeySet};
pub use crate::jwks::verifier::{JwksVerifier, VerifierConfig};
pub use crate::metadata::ProviderMetadata;
pub use crate::provider::{ProviderClaims, ProviderSecret};
