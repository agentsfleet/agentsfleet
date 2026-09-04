//! Every `afd_identity` test file, in one test binary.
//!
//! One binary rather than seven, for the reason `core_suite.rs` records: cargo
//! runs test binaries serially and the tests inside one in parallel, so each
//! extra binary bought a serial stretch and re-paid its own process start.
//!
//! `support` is declared once here and reached as `crate::support` from each
//! suite, rather than three files each declaring their own copy of it.

#[path = "support/mod.rs"]
mod support;

#[path = "cache_refresh.rs"]
mod cache_refresh;
#[path = "capability_windows.rs"]
mod capability_windows;
#[path = "claim_authority_bounds.rs"]
mod claim_authority_bounds;
#[path = "claim_shapes.rs"]
mod claim_shapes;
#[path = "http_key_set.rs"]
mod http_key_set;
#[path = "jwks_verify_negative_paths.rs"]
mod jwks_verify_negative_paths;
#[path = "key_set_parsing.rs"]
mod key_set_parsing;
#[path = "provider_claims.rs"]
mod provider_claims;
#[path = "provider_metadata.rs"]
mod provider_metadata;
