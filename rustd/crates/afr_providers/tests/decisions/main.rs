//! Production decision evaluation against scripted HTTP on isolated TLS sockets.
//! Scenarios protect protocol, accounting and failure behavior rather than fields.
#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test fixtures and assertions fail loudly with their scenario"
)]

mod ambiguity;
mod diagnostics;
mod failures;
mod fixture;
mod limits;
mod preflight_failures;
mod protocol;
mod responses;
#[path = "../support/tls.rs"]
mod tls;
mod trace_fixture;
