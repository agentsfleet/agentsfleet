//! `fleet.events` — every statement that writes the table, in one place.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.
//!
//! # Why the statements are public and the reads are not
//!
//! A write here binds a caller's domain type — an [`Acquired`] lease, a gate's
//! `Refusal`, a runner's verdict — and those types belong to the runner plane.
//! Moving the binding down here would drag them with it and invert the
//! layering, so the writers keep their `bind` chains and this module owns the
//! one text each of them runs. That is the whole of what was wrong before:
//! `afd_approval` carried a byte-identical copy of [`INSERT_FLEET_EVENT`]
//! because reaching the original meant depending on 25,500 lines of runner
//! plane to reuse ten lines of SQL.
//!
//! The READS bind nothing from anywhere else — a filter, a cursor, a limit —
//! so they are verbs on [`History`](crate::History) rather than exported text.
//!
//! # The database owns every stamp and the row id
//!
//! `created_at` and `updated_at` are written by `now()`, and the row's `id` is
//! the column's own `uuidv7()` default, read back with `RETURNING id`. No
//! writer binds an instant or mints an id: an event is named by its
//! `(fleet_id, event_id)` pair, and the row id is a fact the insert reports.
//!
//! [`Acquired`]: https://docs.rs/afd_fleet

/// Record an inbound event.
///
/// The conflict arm on `(fleet_id, event_id)` is the idempotence boundary for
/// redelivery: the same event arriving twice holds one row, so a retrying
/// producer cannot double-run a fleet. It is also what makes the reclaim path
/// safe — a re-leased event finds its row already there — and what makes a
/// retried approval resolve continue its run exactly once.
///
/// The arm converges rather than doing nothing, because two writers race for
/// this row and only one of them knows the predecessor. `COALESCE` keeps a
/// predecessor whichever writer arrives with it, and never overwrites one
/// already stored, so a redelivery that knows nothing cannot clear one.
///
/// `(xmax = 0)` is how a caller tells the arms apart: it is true only for a
/// fresh insert. The fleet's activity counter does not move on the conflict
/// arm either way: `trg_events_bump_count` is `AFTER INSERT` alone
/// (`schema/1770_fleet_activity_counters.sql`). `id` is the row's on both arms.
///
/// The `DO UPDATE` is deliberately UNCONDITIONAL. A filtered `DO UPDATE` that
/// matches no row updates nothing, `RETURNING` then yields nothing, and both
/// callers read this statement with `fetch_one`. Adding that guard turns every
/// redelivery into `RowNotFound` — on the receive path, which decides whether
/// a fleet has already paid.
///
/// `$1` fleet, `$2` event, `$3` workspace, `$4` actor, `$5` type, `$6` body,
/// `$7` resumes-event, `$8` status.
pub const INSERT_FLEET_EVENT: &str = include_str!("insert_fleet_event.sql");

/// [`INSERT_FLEET_EVENT`] as the lease path runs it: a steer's row is stamped
/// with the instant it was ADMITTED, not the instant a runner took it.
///
/// A thread page merges history with the ledger's waiting steers on one key,
/// `(created_at, event_id)`, and a waiting steer sits at its admission time.
/// Stamped at lease, the same message jumped above a page cursor already
/// handed out. Stamped at admission, the row keeps the key it was listed under.
///
/// The instant is the admission row's stored `created_at`, looked up by its
/// primary key — an admitted event's id IS that row's id — while the ledger
/// has not yet stamped it delivered, which a first delivery always matches:
/// the stamp follows this insert. Anything else — a webhook, a schedule, an id
/// that is not a UUID (bound as NULL) — keeps `now()`. Returns the stored
/// `created_at` as well, so the received frame announces the row's own instant.
///
/// `$1`–`$8` as [`INSERT_FLEET_EVENT`], then `$9` the admission row's id, or
/// NULL when the event id is not one, `$10` the steer producer.
pub const INSERT_LEASED_FLEET_EVENT: &str = include_str!("insert_leased_fleet_event.sql");

/// End an event at a gate, naming what refused it.
///
/// Guarded on `status = $5` — always [`status::RECEIVED`] — so a terminal row
/// is NEVER reopened. A request arriving after a `gate_blocked` is a new
/// delivery with its own row (RULE IDMP), not a resurrection of this one.
///
/// No row is not an error: it means the row was already terminal, which
/// happens when a refused delivery's earlier acknowledgement was lost. When the
/// guard matched, the closing carries the terminal row and the fleet facts the
/// live tail's completion frame publishes — its status, its pending gate
/// count, and its two activity counters — so ending a run costs one statement.
/// [`Closed`](crate::Closed) decodes it.
///
/// `NULLIF($6, '')` keeps an empty detail `NULL`, so a consumer testing
/// `IS NULL` cannot be fooled by an empty string.
///
/// `$1` fleet, `$2` event, `$3` new status, `$4` failure reason, `$5` the
/// status this transition is guarded on, `$6` detail, `$7` the gate status that
/// counts as pending.
///
/// [`status::RECEIVED`]: afd_core::event::status::RECEIVED
pub const UPDATE_FLEET_EVENT_FAILURE: &str = include_str!("update_fleet_event_failure.sql");

/// End an event with the runner's verdict.
///
/// The sibling of [`UPDATE_FLEET_EVENT_FAILURE`] and guarded the same way, on
/// `status = $8`. A terminal row is never reopened, so a redelivery whose
/// acknowledgement was lost yields no row and the settled result stands. A
/// check-then-write would let two reports of one event race, and the second
/// would overwrite the first.
///
/// `failure_reason` and `failure_detail` are NULL on a clean run, structurally
/// rather than by convention — the caller's verdict type has nowhere to carry a
/// cause on the succeeded arm. The trace is written here and nowhere else, so
/// a report the fence refuses writes neither the answer nor the trace.
///
/// `$1` fleet, `$2` event, `$3` new status, `$4` response text, `$5` tokens,
/// `$6` wall milliseconds, `$7` failure reason, `$8` the status this transition
/// is guarded on, `$9` failure detail, `$10` the gate status that counts as
/// pending, `$11` the run's tool trace as JSON text, or NULL.
pub const UPDATE_FLEET_EVENT_RESULT: &str = include_str!("update_fleet_event_result.sql");

/// A fleet's activity counters as the database has them, by primary key.
///
/// Answers exactly one row for any fleet id, zeros for a fleet with no counter
/// row: the trigger that maintains the row creates it on the first event, so a
/// fleet that has never run has none. Two correlated subselects rather than a
/// join, and never an inner one: an inner join drops a fleet that has never
/// run, and `LEFT JOIN` plans as a hash join over every counter row.
///
/// `$1` fleet.
pub const SELECT_FLEET_COUNTERS: &str = include_str!("select_fleet_counters.sql");

/// One event's current status.
///
/// Read on ONE path only: a redelivery whose insert hit the conflict arm. The
/// question it answers is whether that redelivery is a legitimate re-poll (the
/// row is still `received`) or an already settled event whose acknowledgement
/// was lost. Re-running the second would double-fire side effects and re-meter
/// tokens, so the two must not be guessed between.
///
/// `$1` fleet, `$2` event.
pub const SELECT_FLEET_EVENT_STATUS: &str = include_str!("select_fleet_event_status.sql");

/// The byte ceiling on a stored failure cause.
///
/// Applied at the WRITE rather than at the read, so a runaway child cannot
/// bloat the row — the cap bounds the row, not the operator's visibility,
/// since the console renders the cause as one line anyway.
pub const MAX_FAILURE_DETAIL_BYTES: usize = 512;
