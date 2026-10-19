//! Every `afd_state` credential-directory suite, in one test binary.
//!
//! Split by credential class to stay under the length cap, and held in one
//! binary so the shared fixtures are one module: three binaries would each see
//! most of `directory_fixtures` as unused. Every suite mints its own rows and
//! looks up only the digests it filed, so they run in parallel against the
//! lane's one database; the two that break a datastore on purpose take a
//! disposable database of their own.
#![cfg(feature = "test-util")]

#[path = "support/directory_fixtures.rs"]
mod support;

#[path = "directory/outages.rs"]
mod outages;
#[path = "directory/people.rs"]
mod people;
#[path = "directory/runners.rs"]
mod runners;
