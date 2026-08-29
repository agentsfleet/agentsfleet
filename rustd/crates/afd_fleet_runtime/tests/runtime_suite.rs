//! Every `afd_fleet_runtime` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. The audit preceding every aggregation on this branch
//! ran here too: no suite asserts over global state — no `total()`, `COUNT(`
//! or unfiltered listing over a shared table — so concurrency between these
//! suites has nothing to race on. The support module is declared once and
//! reached as `crate::support`.

#[path = "support/mod.rs"]
// The helper's panic was covered by the declaring suite's own `#![expect]`
// while that suite was a crate root; the allowance now rides the declaration.
#[allow(
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod support;

#[path = "frontmatter_corpus.rs"]
mod frontmatter_corpus;
#[path = "frontmatter_fields.rs"]
mod frontmatter_fields;
