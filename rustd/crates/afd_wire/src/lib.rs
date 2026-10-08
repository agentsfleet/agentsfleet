//! The `/v1/runners` protocol the daemon serves and the runner consumes.
//!
//! Only what both sides speak lives here. The daemon's own API types (admin,
//! tenant, operator, ingress) live in `afd_api_wire`, which depends on this
//! crate and never the reverse, so an edit there rebuilds no runner crate.
//!
//! These types ARE the wire. `agentsfleetd` publishes them through
//! `public/openapi.json` (the `openapi` feature derives the schemas), and
//! `agentsfleet-runner` is a client of that document: the `afr_*` crates
//! decode these same types, so what is published here is what both sides speak.
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
//! Identifiers are `Cow<'a, str>` and counts are plain integers — this crate
//! does NOT depend on `afd_core`. Validation belongs at the service boundary,
//! and doing it at parse would break the thing this layer exists to guarantee:
//! `afd_core::limits::WorkerCount` clamps on deserialize, so a payload carrying
//! `worker_count: 168` would decode to `64` and re-serialize to `64` — a byte
//! mismatch against a fixture that carries `168`, because the daemon clamps at
//! assignment rather than at parse.
//!
//! # `skip_serializing_if` is for receivers that refuse unknown fields
//!
//! An absent optional writes `null`, except on a shape whose receiver is
//! `#[serde(deny_unknown_fields)]`, such as the report and the activity
//! frames. There a field added after release skips itself when absent, so a
//! message that does not use it still decodes on a receiver that predates it.
//! A shape that admits unknown fields, such as [`lease::LeasePayload`], never
//! skips: its newer fields decode from absence through `#[serde(default)]`.
//!
//! # Version
//!
//! This is the CURRENT lease shape only. One lease version is on the wire, and
//! a second lease type here would be a second implementation of the same verb
//! rather than a field added to this one.

// A dependency listed but unused is a supply-chain and compile-time cost with no
// offsetting benefit, and an unused-but-linked runtime is how this crate would
// breach the no-runtime invariant. Gated on `not(test)` because the test build
// links dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod activity;
pub mod credentials;
pub mod event;
pub mod lease;
pub mod memory;
pub mod message_verb;
pub mod paths;
pub mod policy;
pub mod redact;
pub mod report;
pub mod runner;
pub mod schedule_verb;
pub mod tool_detail;
pub mod tool_trace;
