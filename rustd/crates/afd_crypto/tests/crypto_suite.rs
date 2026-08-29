//! Every `afd_crypto` test file, in one test binary.
//!
//! One binary rather than 8: cargo runs test BINARIES serially and the tests
//! inside one binary in parallel, so each extra binary bought a serial stretch
//! and re-paid its own process start and dynamic linking.
//!
//! Safe to aggregate because these suites share no datastore, and touch no live Postgres or Redis at all. That is
//! the check aggregation actually turns on, and it is not a formality: doing
//! this to `afd_fleet` made eighteen suites concurrent against one Postgres and
//! broke a test asserting a global row count held still across a paginated
//! walk. Crates whose suites take `TestDatabase::shared` — `afd_runner` and
//! `afd_tenant` — are deliberately NOT aggregated for that reason.

#[path = "aad.rs"]
mod aad;
#[path = "entropy_mock.rs"]
mod entropy_mock;
#[path = "envelope.rs"]
mod envelope;
#[path = "error_surface.rs"]
mod error_surface;
#[path = "known_answer.rs"]
mod known_answer;
#[path = "mac.rs"]
mod mac;
#[path = "secret.rs"]
mod secret;
#[path = "zig_parity.rs"]
mod zig_parity;
