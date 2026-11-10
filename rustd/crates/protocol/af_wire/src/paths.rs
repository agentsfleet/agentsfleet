//! Wire paths, the runner token prefix, and the lease wire version.
//!
//! Single-sourced here because the router and every client must spell a path
//! identically, and a path built by concatenation at two call sites is two
//! paths that drift.
//!
//! Each route is composed with `concatcp!` from the segments below, so the
//! daemon's router, its published API document and the runner's client read
//! one spelling. A segment renamed here moves every route under it.

use const_format::concatcp;

/// Machine-owned runner path prefix.
pub const RUNNERS: &str = "/v1/runners";

/// Platform operator runner administration.
pub const ADMIN_RUNNERS: &str = "/v1/admin/runners";

/// Runner-token prefix. The wire contract for the machine principal — the daemon
/// mints and validates it, and the host checks it before the lease loop.
pub const RUNNER_TOKEN_PREFIX: &str = "agt_r";

/// `GET /v1/runners/me` — read-only self status, which does not bump liveness.
pub const RUNNER_SELF: &str = concatcp!(RUNNERS, "/me");

/// Four independently flow-controlled lanes on one authenticated native connection.
pub const RUNNER_STREAM: &str = concatcp!(RUNNER_SELF, "/stream");

/// Collection prefix for fenced per-lease operations; the collection accepts no POST.
pub const RUNNER_LEASES: &str = concatcp!(RUNNER_SELF, "/leases");

/// An authoring lease has no runtime tools, credentials or event capabilities.
pub const AUTHORING_LEASE: &str = concatcp!(RUNNER_SELF, "/authoring-leases/{lease_id}/");

/// `GET`/`POST /v1/runners/me/memory/{fleet_id}` — durable fleet memory.
///
/// Collection prefix; the caller appends the `{fleet_id}` segment.
pub const RUNNER_MEMORY: &str = concatcp!(RUNNER_SELF, "/memory");

/// Trailing segment of `POST /v1/runners/me/memory/{fleet_id}/recall` — a
/// search past the hydration window. Bare for the reason
/// [`LEASE_ACTIVITY_SUFFIX`] gives.
pub const RUNNER_MEMORY_RECALL_SUFFIX: &str = "recall";

/// `POST /v1/runners/me/credentials/mint` — on-demand credential mint.
pub const RUNNER_CREDENTIALS_MINT: &str = concatcp!(RUNNER_SELF, "/credentials/mint");

/// Trailing segment of the per-lease activity sub-resource.
///
/// A bare segment rather than a joined constant: `lease_id` is a path parameter,
/// so the full path is `{RUNNER_LEASES}/{lease_id}/{LEASE_ACTIVITY_SUFFIX}`.
pub const LEASE_ACTIVITY_SUFFIX: &str = "activity";

/// Trailing segment of the per-lease tool-call records sub-resource. See
/// [`LEASE_ACTIVITY_SUFFIX`] for why it is a bare segment.
pub const LEASE_TOOL_CALLS_SUFFIX: &str = "tool-calls";

/// Trailing segment of the per-lease schedules sub-resource: the schedules of
/// the fleet the lease runs. See [`LEASE_ACTIVITY_SUFFIX`] for why it is a
/// bare segment.
pub const LEASE_SCHEDULES_SUFFIX: &str = "schedules";

/// Trailing segment of one schedule's runs: its history, and where a run is
/// created to fire it now. Bare, beneath `{LEASE_SCHEDULES_SUFFIX}/{schedule_id}`.
pub const SCHEDULE_RUNS_SUFFIX: &str = "runs";

/// Trailing segment of the per-lease messages sub-resource: a line said to
/// the event's thread before the answer. See [`LEASE_ACTIVITY_SUFFIX`].
pub const LEASE_MESSAGES_SUFFIX: &str = "messages";

/// One held lease, the root every per-lease route sits under. The router
/// matches `{lease_id}`; the runner puts the lease's id in its place.
const LEASE: &str = concatcp!(RUNNER_LEASES, "/{lease_id}/");

/// One fleet's memory: `GET` hydrates it, `POST` captures into it.
pub const RUNNER_MEMORY_FLEET: &str = concatcp!(RUNNER_MEMORY, "/{fleet_id}");

/// `POST` — a search of one fleet's memory past the hydration window.
pub const RUNNER_MEMORY_RECALL: &str =
    concatcp!(RUNNER_MEMORY_FLEET, "/", RUNNER_MEMORY_RECALL_SUFFIX);

/// `POST` — finished calls' full records for a held lease.
pub const LEASE_TOOL_CALLS: &str = concatcp!(LEASE, LEASE_TOOL_CALLS_SUFFIX);

/// `GET`/`POST` — the leased fleet's schedules.
pub const LEASE_SCHEDULES: &str = concatcp!(LEASE, LEASE_SCHEDULES_SUFFIX);

/// `PATCH`/`DELETE` — one schedule the leased fleet made.
pub const LEASE_SCHEDULE: &str = concatcp!(LEASE_SCHEDULES, "/{schedule_id}");

/// `GET`/`POST` — one schedule's runs.
pub const LEASE_SCHEDULE_RUNS: &str = concatcp!(LEASE_SCHEDULE, "/", SCHEDULE_RUNS_SUFFIX);

/// `POST` — a line said to the leased event's thread.
pub const LEASE_MESSAGES: &str = concatcp!(LEASE, LEASE_MESSAGES_SUFFIX);

/// Private working notes for one branch of a live lease.
pub const LEASE_CONTEXT: &str = concatcp!(LEASE, "context-checkpoints/{branch_id}");

/// Physical decision reservations owned by the existing fenced lease.
pub const LEASE_TASK_DECISIONS: &str = concatcp!(LEASE, "tasks/decisions");

/// Original-owner accounting for one reserved physical decision call.
pub const LEASE_TASK_DECISION: &str = concatcp!(LEASE_TASK_DECISIONS, "/{call_id}");

/// Complete pending human messages under the current lease.
pub const LEASE_TASK_INPUTS: &str = concatcp!(LEASE, "tasks/inputs");
/// Complete immutable interpretation of an admitted human message.
pub const LEASE_TASK_BINDINGS: &str = concatcp!(LEASE, "tasks/bindings");
/// Immutable native publication collection and recovery watermark reads.
pub const LEASE_TASK_EFFECTS: &str = concatcp!(LEASE, "tasks/effects");
/// One native publication and its attributable remote receipt.
pub const LEASE_TASK_EFFECT: &str = concatcp!(LEASE_TASK_EFFECTS, "/{effect_id}");

/// Original source reply or current-boundary input application acknowledgement.
pub const LEASE_TASK_INPUT_RECEIPTS: &str = concatcp!(LEASE, "tasks/input-receipts");
/// Native immutable capture plan allocation.
pub const LEASE_TASK_CHECKPOINTS: &str = concatcp!(LEASE, "tasks/checkpoints");
/// Complete compatible manifest publication.
pub const LEASE_TASK_CHECKPOINT: &str = concatcp!(LEASE_TASK_CHECKPOINTS, "/{checkpoint_id}");
/// One immutable bounded base/overlay page.
pub const LEASE_TASK_CHECKPOINT_PAGE: &str = concatcp!(LEASE_TASK_CHECKPOINT, "/pages/{page_id}");
/// Native object registration and bounded restore reads.
pub const LEASE_TASK_CHECKPOINT_OBJECTS: &str = concatcp!(LEASE_TASK_CHECKPOINT, "/objects");
/// Immutable ciphertext descriptors and conditional upload authorizations.
pub const LEASE_TASK_CHECKPOINT_UPLOADS: &str = concatcp!(LEASE_TASK_CHECKPOINT, "/uploads");
/// Restore-pin bootstrap and bounded transfer refresh.
pub const LEASE_TASK_CHECKPOINT_AUTHORIZATIONS: &str =
    concatcp!(LEASE_TASK_CHECKPOINT, "/authorizations");
/// One native reader/writer pin renewal.
pub const LEASE_TASK_CHECKPOINT_PIN: &str = concatcp!(LEASE_TASK_CHECKPOINT, "/pins/{pin_id}");
