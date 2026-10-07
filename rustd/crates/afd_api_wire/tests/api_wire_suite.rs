//! Every `afd_api_wire` test file, in one test binary.
//!
//! One binary for the reason `afd_wire`'s `wire_suite.rs` gives: cargo runs
//! test binaries serially, and these suites share no datastore.

#[path = "admin_shapes.rs"]
mod admin_shapes;
// Ungated: it reads both crates' sources, not the schemas.
#[path = "names.rs"]
mod names;
#[path = "redaction.rs"]
mod redaction;
// Gated with the feature it grades: without `openapi` there are no schemas.
#[cfg(feature = "openapi")]
#[path = "schema.rs"]
mod schema;
#[path = "tenant_provider_shapes.rs"]
mod tenant_provider_shapes;
