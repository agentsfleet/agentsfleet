//! Every `afd_tenant` test file, in one test binary.
//!
//! # Why a SHARED database is not a reason to skip this crate
//!
//! `afd_fleet` broke when aggregation made its suites concurrent, so the audit
//! since has asked what a crate's suites share. The first reading of that audit
//! treated `TestDatabase::shared` as disqualifying and left this crate alone.
//! That was too blunt. `support/apikey_lane.rs` says what the design actually
//! is: the database is shared BECAUSE "two suites running in parallel address
//! the same tables", and "each mints its own tenant, and every read here is
//! scoped by it". Parallel suites are the case it was written for.
//!
//! What broke `afd_fleet` was not sharing. It was sharing plus an assertion over
//! GLOBAL state — a row count held still across a paginated walk. There is no
//! such assertion here: no `total()`, no `COUNT(`, no bare-length check over a
//! shared table. Sharing with scoped reads survives concurrency; sharing with a
//! global assertion does not.
//!
//! # Three helpers that were all called `support`
//!
//! Each suite declared its own helper under the name `support`, and they were
//! three DIFFERENT files — `apikey_lane`, `redis_harness`, `preference_lane`.
//! One binary means one namespace, so each is declared here under the name of
//! the file it actually is. That is the only edit the suites needed.

// The helpers carry no lint attributes of their own: each inherited them from
// whichever suite declared it, back when that suite WAS a crate root. Declared
// at this root, the allowance travels with the declaration -- scoped to the
// helper rather than blanketed over the suites, which would hand them
// permissions their own headers deliberately withhold.
#[path = "support/apikey_lane.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod apikey_lane;
#[path = "support/preference_lane.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod preference_lane;
#[path = "support/redis_harness.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod redis_harness;

#[path = "integration_api_key_paging.rs"]
mod integration_api_key_paging;
#[path = "integration_device_flow.rs"]
mod integration_device_flow;
#[path = "integration_preferences.rs"]
mod integration_preferences;
#[path = "integration_signup.rs"]
mod integration_signup;
#[path = "integration_workspace_ownership.rs"]
mod integration_workspace_ownership;
