//! The names `schema/**.sql` writes as literals and the daemon writes as code.
//!
//! Postgres parses a trigger body, a partial-index predicate and a
//! `current_setting()` name at DDL time. None of the three can reference a Rust
//! constant, so the schema has no choice but to spell these words itself — and
//! that is exactly the drift RULE STS exists to catch. A `DEFAULT` that drifts
//! shows up as wrong data. A setting name that drifts fails *silently*: the
//! guard simply stops guarding, and nothing in the system is louder about it.
//!
//! The schema still writes the literal; every Rust reader spells it from here,
//! so the Rust side has one place to rename. The schema side is held by
//! convention — no test pins the two spellings to each other.

/// The transaction-scoped setting that lets a hard purge past the append-only
/// triggers.
///
/// Seven schema files read it — the approval-gate tables and every repair table
/// — and each writes the name as a literal inside its own trigger body. Rust
/// sets it in one place, `afd_fleet_lifecycle::sql::ALLOW_GATE_PURGE`. Rename it
/// on one side only and every append-only trigger starts refusing the cascade,
/// which means a personal-account erasure stops working with no error anyone
/// reads and no failing test.
pub const GATE_PURGE_SETTING: &str = "fleet.allow_gate_purge";

/// The value that setting must carry for a purge to be admitted.
pub const GATE_PURGE_ENABLED: &str = "on";
