//! `core.fleet_sessions` — the cursor a fleet resumes from.
//!
//! Split from the event statements when `core.fleet_events` moved to
//! `afd_events`. This is a DIFFERENT table with a different lifetime: an event
//! row records that work happened, this row records where the conversation got
//! to, and only the lease's finalize path ever writes it. It stayed here
//! because it is the runner plane's, and moving it would have given the event
//! crate a table no operator surface reads.
//!
//! Text is byte-identical to `fleet/sql.zig`.

/// The byte ceiling on a stored checkpoint response.
///
/// `event_rows.zig`'s `MAX_CHECKPOINT_RESPONSE_BYTES`. Four times
/// `afd_events::sql::MAX_FAILURE_DETAIL_BYTES` because this one is the resume
/// cursor a session continues from, where a cause line only has to be readable.
pub const MAX_CHECKPOINT_RESPONSE_BYTES: usize = 2048;

/// Checkpoint a fleet's session — one row per fleet, replaced in place.
///
/// `fleet/sql.zig`'s `UPSERT_FLEET_SESSION`. The cursor is the whole value: a
/// fleet resumes from `last_event_id` and `last_response`, so this row is what
/// makes a session continuous across runs on different runners.
///
/// Replaced rather than appended because only the LATEST checkpoint is ever
/// read. A history table here would grow per run and be queried never.
///
/// `$1` fleet, `$2` the context document, `$3` now.
pub const UPSERT_FLEET_SESSION: &str = "\
INSERT INTO core.fleet_sessions (fleet_id, context_json, checkpoint_at, created_at, updated_at)
VALUES ($1::uuid, $2::jsonb, $3, $3, $3)
ON CONFLICT (fleet_id) DO UPDATE
  SET context_json = EXCLUDED.context_json,
      checkpoint_at = EXCLUDED.checkpoint_at,
      updated_at = EXCLUDED.updated_at";
