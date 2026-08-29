//! Every `afd_redis` test file, in one test binary.
//!
//! Eleven binaries became one, for the reason the other suites record: cargo
//! runs test binaries serially and the tests inside one in parallel.
//!
//! # Why this crate is safe to aggregate
//!
//! What aggregation turns on is CONCURRENCY between a crate's suites, and the
//! audit that precedes it asks what they share. Here:
//!
//! - No suite asserts a global count. There is no `FLUSHALL`, `FLUSHDB`, `KEYS`
//!   or `DBSIZE` anywhere; `RedisHarness` namespaces every key behind a prefix
//!   minted from the process id and a counter, exactly so parallel tests never
//!   collide. That is the failure `afd_fleet` hit and this crate cannot.
//! - The two subscriber installers do not fight. `support/subscriber.rs` calls
//!   `set_global_default` once for the process; `support/recorder.rs` calls
//!   `set_default`, which is THREAD-local and returns a guard. One being global
//!   and the other scoped is why they coexist in a single binary.
//! - `RECORDER_SERIAL` is held only by suites that install a recorder, which is
//!   `misbehaving_server` alone, so its contention set does not widen here.
//! - `HUB_LANE` serialises the hub suite against itself, which aggregation does
//!   not change.
//! - Both fake servers bind `("127.0.0.1", 0)`, so no fixed port can collide.
//!
//! # Nested modules stay where they are
//!
//! `redis_harness` declares its own `subscriber`, and `fake_redis` its own
//! `resp`, through `#[path]` attributes that resolve against the directory of
//! the file declaring them — `tests/support/`. Aggregation does not move those
//! files, so those paths keep resolving and are deliberately not hoisted.

// The helpers carry no lint attributes of their own: each inherited them from
// whichever suite declared it, back when that suite WAS a crate root. They are
// declared at this root now, so the allowance travels with the declaration --
// scoped to the helpers rather than blanketed over every suite, which would
// hand the suites permissions their own headers deliberately withhold.
//
// `subscriber` is declared here exactly once. `fake_redis` and `redis_harness`
// each used to declare it, which was fine while no single binary loaded both,
// and is a duplicate module the moment one does.
#[path = "support/subscriber.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod subscriber;

#[path = "support/fake_redis.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod fake_redis;

#[path = "support/recorder.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod recorder;

#[path = "support/redis_harness.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod support;

#[path = "connect_refusals.rs"]
mod connect_refusals;
#[path = "diagnose_connect.rs"]
mod diagnose_connect;
#[path = "error_surface.rs"]
mod error_surface;
#[path = "hub_socket_faults.rs"]
mod hub_socket_faults;
#[path = "integration_hub.rs"]
mod integration_hub;
#[path = "integration_ready.rs"]
mod integration_ready;
#[path = "integration_session.rs"]
mod integration_session;
#[path = "integration_streams.rs"]
mod integration_streams;
#[path = "keys_and_config.rs"]
mod keys_and_config;
#[path = "misbehaving_server.rs"]
mod misbehaving_server;
#[path = "verify_outcomes.rs"]
mod verify_outcomes;
