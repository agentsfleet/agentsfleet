//! Serde port of the frozen `/v1/runners` protocol the daemon and runner exchange.
//!
//! The Zig module `src/lib/contract` is the source of truth for this wire. These
//! types conform to it and never the other way round: `src/lib/contract/fixture_export.zig`
//! emits one canonical JSON document per exported type into
//! `samples/fixtures/wire-v2/`, and the round-trip suite parses each one,
//! re-serializes it, and compares BYTES. A field renamed, reordered, retyped or
//! dropped on either side turns that comparison red.
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
pub mod auth;
pub mod credentials;
pub mod event;
pub mod lease;
pub mod memory;
pub mod models;
pub mod paths;
pub mod policy;
mod redact;
pub mod report;
pub mod runner;
pub mod tenant;
pub mod workspace;
