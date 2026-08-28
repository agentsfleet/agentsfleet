//! The signature wall — what an unauthenticated delivery must prove before
//! anything reads its body.
//!
//! # What this crate is, and firmly is not
//!
//! It answers one question: do these bytes prove they came from the holder of
//! this secret? It opens no socket, reads no row, and knows nothing about HTTP
//! beyond the names of a few headers. Every branch is therefore provable with
//! no datastore, no runtime and no router — which is the property the wall
//! needs most, because a verification path that can only be tested through a
//! live request is one whose failure modes go untested.
//!
//! It is deliberately NOT part of `afd_auth`. That crate answers "who is this
//! caller and what may they do" — principals, planes, scope rungs. A signed
//! delivery has no principal at all: `route/webhook.rs` says it outright —
//! *the sender proves itself, not a person*. Two different questions, two
//! crates, and neither grows the other's dependencies.
//!
//! # The paths
//!
//! ```text
//!   delivery
//!      │
//!      ├── Scheme::BodyHex      hex(HMAC(secret, body))           GitHub
//!      ├── Scheme::BodyHexBare  the same, no prefix               Linear
//!      ├── Scheme::SlackV0      hex(HMAC(secret, v0:ts:body))     Slack
//!      └── vendor::svix         b64(HMAC(secret, id.ts.body))     Svix/Clerk
//!                │
//!                ▼
//!          HmacSha256Tag::verify        constant-time, one implementation
//!                │
//!                ▼
//!             Verdict                   Verified | Refused(Refusal)
//! ```
//!
//! Two schemes that look alike are still separate arms rather than rows in a
//! config table, because canonicalisation is the part of webhook verification
//! that has to be READ. See [`scheme`] for the full argument.
//!
//! The `QStash` delivery verifier is not here: it is a JWT rather than a body
//! signature, and it lives with the rest of that vendor's protocol in
//! `afd_qstash`.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod freshness;
pub mod scheme;
pub mod vendor;
pub mod verdict;

pub use self::freshness::MAX_DRIFT_SECONDS;
pub use self::scheme::Scheme;
pub use self::verdict::{Refusal, Verdict};
