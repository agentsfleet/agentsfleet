//! The `/v1/runners` protocol the daemon serves and the runner consumes.
//!
//! These types ARE the wire. `agentsfleetd` publishes them through
//! `public/openapi.json` (the `openapi` feature derives the schemas), and
//! `agentsfleet-runner` is a client of that document: its Zig structs in
//! `src/lib/contract` conform to what is published here, never the reverse.
//!
//! # Borrowed, not owned
//!
//! Text fields are `Cow<'a, str>` behind `#[serde(borrow)]` rather than `String`.
//! Every lease, report, heartbeat and activity frame crosses this layer, so the
//! common case — a payload with no JSON escapes — parses without allocating a
//! single field, while an escaped string still decodes correctly by falling back
//! to an owned copy. `String` everywhere would allocate per field per request and
//! is the one decision here that is expensive to reverse later.
//!
//! # Primitives, not validated newtypes
//!
//! Identifiers are `Cow<'a, str>` and counts are plain integers, matching the
//! Zig structs field for field — this crate does NOT depend on `afd_core`.
//! Validation belongs at the service boundary, and doing it at parse would
//! break the thing this layer exists to guarantee: `afd_core::limits::WorkerCount`
//! clamps on deserialize, so a payload carrying `worker_count: 168` would decode
//! to `64` and re-serialize to `64` — a byte mismatch against a fixture the Zig
//! daemon, which clamps at assignment rather than at parse, emits as `168`.
//!
//! # No `skip_serializing_if`, anywhere
//!
//! The Zig emitter writes `null` for an absent optional, so serde must too. A
//! `skip_serializing_if` would drop the key and break byte equality — which is
//! why the round-trip test exists rather than a field-by-field comparison that
//! would not notice.
//!
//! # Version
//!
//! This is the CURRENT lease shape only. The Zig daemon carries a superseded
//! version-one lease alongside it; the port does not, and the fixture manifest
//! records that exclusion so an accidental re-admission fails a test rather than
//! quietly growing a second implementation.

// A dependency listed but unused is a supply-chain and compile-time cost with no
// offsetting benefit, and an unused-but-linked runtime is how this crate would
// breach the no-runtime invariant. Gated on `not(test)` because the test build
// links dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod activity;
pub mod admin;
mod admin_catalogue;
mod admin_library;
pub mod approval;
pub mod auth;
pub mod connector;
pub mod credentials;
pub mod event;
pub mod fleet;
pub mod grant;
pub mod health;
pub mod ingress;
pub mod lease;
pub mod memory;
pub mod models;
pub mod operator;
pub mod paths;
pub mod policy;
pub mod preference;
mod redact;
pub mod report;
pub mod runner;
pub mod schedule;
pub mod schema;
pub mod secret;
pub mod tail;
pub mod tenant;
pub mod tenant_model_entry;
pub mod tenant_provider;
pub mod workspace;
pub mod workspace_library;
