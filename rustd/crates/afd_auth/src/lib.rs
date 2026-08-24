//! Who the caller is, and what they may do.
//!
//! Two independent axes decide a request. This crate owns both halves of the
//! first — proving a credential, and checking the CAPABILITY it resolves to
//! against the scope catalogue in [`scope`]. Ownership — whether the
//! principal's tenant owns the target workspace — is a separate check that
//! composes with it, because holding `fleet:write` must not let a caller touch
//! a workspace they do not own.
//!
//! The former `user < operator < admin` role ladder and the `platform_admin`
//! flag are gone from the product; they were undocumented capability bundles.
//! See `docs/AUTH.md` §Scope catalogue for the vocabulary and the provisioning
//! grants, and `src/agentsfleetd/auth/scopes.zig` for the canon this mirrors.
//!
//! # The shape, and why it is not the Zig daemon's
//!
//! `bearer_or_api_key.zig` routes with a chain of `if`, and the three classes
//! it routes to are three hand-written procedures differing only in constants.
//! Both facts follow from one root cause, so both are fixed by one change:
//!
//! ```text
//!   Authorization: Bearer …
//!         │
//!         ▼
//!   Presented                 non-empty · redacting Debug · zeroed on drop
//!         │
//!         ▼
//!   CredentialKind::of        a prefix TABLE, proven prefix-free at build time
//!         │
//!         ▼
//!   Plane::admits             the runner/tenant boundary, as data not wiring
//!         │
//!         ▼
//!   Registry::authenticate    a TOTAL match — a new class fails the build
//!         │
//!         ├── stored ─── one procedure + a HashedClass const per class
//!         │                 CredentialDirectory · CapabilitySource
//!         └── token  ─── TokenVerifier; the claim rides the credential
//!         │
//!         ▼
//!   Principal                 enum-with-data — illegal states unspellable
//!         │
//!         ▼
//!   require_scope             any-of, hierarchy-expanded, 403 UZ-AUTH-022
//! ```
//!
//! The three traits are the seams, and they are seams for the reason Zig's
//! injected `LookupFn` and `ScopeFn` are: each reaches a network or a
//! datastore, and an authentication decision must be provable without either.
//! Under `test-util` this crate ships in-memory implementations of all three,
//! so every branch above is exercised with no runtime dependency at all.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target, where a test-only crate legitimately
// goes unused by the library's own code.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod authenticate;
pub mod capability;
pub mod credential;
pub mod directory;
pub mod error;
pub mod gate;
pub mod plane;
pub mod principal;
pub mod scope;
pub mod verifier;

#[cfg(feature = "test-util")]
pub mod mock;

pub use crate::authenticate::Registry;
pub use crate::capability::{CapabilitySource, NoCapabilitySource};
pub use crate::credential::{Blank, CredentialKind, Presented};
pub use crate::directory::{CredentialDirectory, CredentialRecord, Digest, Liveness};
pub use crate::error::{AuthError, Unavailable};
pub use crate::gate::{Denied, require_scope};
pub use crate::plane::Plane;
pub use crate::principal::{Person, PersonCredential, Principal, Runner, Subject};
pub use crate::scope::{Scope, ScopeSet};
pub use crate::verifier::{NoVerifier, TokenVerifier, VerifiedClaims, VerifyError};
