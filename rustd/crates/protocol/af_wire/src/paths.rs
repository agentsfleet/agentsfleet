//! Wire paths, the runner token prefix, and the run wire version.
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

/// A tenant's own runners.
pub const TENANT_RUNNERS: &str = "/v1/tenants/me/runners";

/// One of the calling tenant's runners.
pub const TENANT_RUNNER: &str = concatcp!(TENANT_RUNNERS, "/{runner_id}");

/// Runner-token prefix. The wire contract for the machine principal — the daemon
/// mints and validates it, and the host checks it before the run loop.
pub const RUNNER_TOKEN_PREFIX: &str = "agt_r";

/// `GET /v1/runners/me` — read-only self status, which does not bump liveness.
pub const RUNNER_SELF: &str = concatcp!(RUNNERS, "/me");

/// Four independently flow-controlled lanes on one authenticated native connection.
pub const RUNNER_STREAM: &str = concatcp!(RUNNER_SELF, "/stream");

/// Collection prefix for fenced per-run operations; the collection accepts no POST.
pub const RUNNER_RUNS: &str = concatcp!(RUNNER_SELF, "/runs");

/// `GET`/`POST /v1/runners/me/memory/{fleet_id}` — durable fleet memory.
///
/// Collection prefix; the caller appends the `{fleet_id}` segment.
pub const RUNNER_MEMORY: &str = concatcp!(RUNNER_SELF, "/memory");

/// Trailing segment of `POST /v1/runners/me/memory/{fleet_id}/recall` — a
/// search past the hydration window. Bare for the reason
/// [`RUN_ACTIVITY_SUFFIX`] gives.
pub const RUNNER_MEMORY_RECALL_SUFFIX: &str = "recall";

/// `POST /v1/runners/me/credentials/mint` — on-demand credential mint.
pub const RUNNER_CREDENTIALS_MINT: &str = concatcp!(RUNNER_SELF, "/credentials/mint");

/// Trailing segment of the per-run activity sub-resource.
///
/// A bare segment rather than a joined constant: `run_id` is a path parameter,
/// so the full path is `{RUNNER_RUNS}/{run_id}/{RUN_ACTIVITY_SUFFIX}`.
pub const RUN_ACTIVITY_SUFFIX: &str = "activity";

/// Trailing segment of the per-run tool-call records sub-resource. See
/// [`RUN_ACTIVITY_SUFFIX`] for why it is a bare segment.
pub const RUN_TOOL_CALLS_SUFFIX: &str = "tool-calls";

/// Trailing segment of the per-run schedules sub-resource: the schedules of
/// the fleet the run runs. See [`RUN_ACTIVITY_SUFFIX`] for why it is a
/// bare segment.
pub const RUN_SCHEDULES_SUFFIX: &str = "schedules";

/// Trailing segment of one schedule's runs: its history, and where a run is
/// created to fire it now. Bare, beneath `{RUN_SCHEDULES_SUFFIX}/{schedule_id}`.
pub const SCHEDULE_RUNS_SUFFIX: &str = "runs";

/// Trailing segment of the per-run messages sub-resource: a line said to
/// the event's thread before the answer. See [`RUN_ACTIVITY_SUFFIX`].
pub const RUN_MESSAGES_SUFFIX: &str = "messages";

/// Trailing segment of the per-run steers sub-resource: work the held
/// fleet puts on another fleet in its workspace. See [`RUN_ACTIVITY_SUFFIX`].
pub const RUN_STEERS_SUFFIX: &str = "steers";

/// Trailing segment of the per-run fleets sub-resource: the fleets of the
/// held fleet's workspace. See [`RUN_ACTIVITY_SUFFIX`].
pub const RUN_FLEETS_SUFFIX: &str = "fleets";

/// Trailing segment of one fleet's recent events, beneath
/// `{RUN_FLEETS_SUFFIX}/{fleet_id}`.
pub const FLEET_EVENTS_SUFFIX: &str = "events";

/// One held run, the root every per-run route sits under. The router
/// matches `{run_id}`; the runner puts the run's id in its place.
const RUN: &str = concatcp!(RUNNER_RUNS, "/{run_id}/");

/// One fleet's memory: `GET` hydrates it, `POST` captures into it.
pub const RUNNER_MEMORY_FLEET: &str = concatcp!(RUNNER_MEMORY, "/{fleet_id}");

/// `POST` — a search of one fleet's memory past the hydration window.
pub const RUNNER_MEMORY_RECALL: &str =
    concatcp!(RUNNER_MEMORY_FLEET, "/", RUNNER_MEMORY_RECALL_SUFFIX);

/// `POST` — finished calls' full records for a held run.
pub const RUN_TOOL_CALLS: &str = concatcp!(RUN, RUN_TOOL_CALLS_SUFFIX);

/// `GET`/`POST` — the held fleet's schedules.
pub const RUN_SCHEDULES: &str = concatcp!(RUN, RUN_SCHEDULES_SUFFIX);

/// `PATCH`/`DELETE` — one schedule the held fleet made.
pub const RUN_SCHEDULE: &str = concatcp!(RUN_SCHEDULES, "/{schedule_id}");

/// `GET`/`POST` — one schedule's runs.
pub const RUN_SCHEDULE_RUNS: &str = concatcp!(RUN_SCHEDULE, "/", SCHEDULE_RUNS_SUFFIX);

/// `POST` — a line said to the held event's thread.
pub const RUN_MESSAGES: &str = concatcp!(RUN, RUN_MESSAGES_SUFFIX);

/// `POST` — work the held fleet puts on another fleet in its workspace.
pub const RUN_STEERS: &str = concatcp!(RUN, RUN_STEERS_SUFFIX);

/// `GET` — the fleets of the held fleet's workspace.
pub const RUN_FLEETS: &str = concatcp!(RUN, RUN_FLEETS_SUFFIX);

/// `GET` — one fleet's recent events, as the held fleet reads them.
pub const RUN_FLEET_EVENTS: &str = concatcp!(RUN_FLEETS, "/{fleet_id}/", FLEET_EVENTS_SUFFIX);

/// Physical decision reservations owned by the existing fenced run.
pub const RUN_TASK_DECISIONS: &str = concatcp!(RUN, "tasks/decisions");

/// Original-owner accounting for one reserved physical decision call.
pub const RUN_TASK_DECISION: &str = concatcp!(RUN_TASK_DECISIONS, "/{call_id}");

/// Complete pending human messages under the current run.
pub const RUN_TASK_INPUTS: &str = concatcp!(RUN, "tasks/inputs");
/// Complete immutable interpretation of an admitted human message.
pub const RUN_TASK_BINDINGS: &str = concatcp!(RUN, "tasks/bindings");
/// Immutable native publication collection and recovery watermark reads.
pub const RUN_TASK_EFFECTS: &str = concatcp!(RUN, "tasks/effects");
/// One native publication and its attributable remote receipt.
pub const RUN_TASK_EFFECT: &str = concatcp!(RUN_TASK_EFFECTS, "/{effect_id}");

/// Original source reply or current-boundary input application acknowledgement.
pub const RUN_TASK_INPUT_RECEIPTS: &str = concatcp!(RUN, "tasks/input-receipts");
/// Native immutable capture plan allocation.
pub const RUN_TASK_CHECKPOINTS: &str = concatcp!(RUN, "tasks/checkpoints");
/// Complete compatible manifest publication.
pub const RUN_TASK_CHECKPOINT: &str = concatcp!(RUN_TASK_CHECKPOINTS, "/{checkpoint_id}");
/// One immutable bounded base/overlay page.
pub const RUN_TASK_CHECKPOINT_PAGE: &str = concatcp!(RUN_TASK_CHECKPOINT, "/pages/{page_id}");
/// Native object registration and bounded restore reads.
pub const RUN_TASK_CHECKPOINT_OBJECTS: &str = concatcp!(RUN_TASK_CHECKPOINT, "/objects");
/// Immutable ciphertext descriptors and conditional upload authorizations.
pub const RUN_TASK_CHECKPOINT_UPLOADS: &str = concatcp!(RUN_TASK_CHECKPOINT, "/uploads");
/// Restore-pin bootstrap and bounded transfer refresh.
pub const RUN_TASK_CHECKPOINT_AUTHORIZATIONS: &str =
    concatcp!(RUN_TASK_CHECKPOINT, "/authorizations");
/// One native reader/writer pin renewal.
pub const RUN_TASK_CHECKPOINT_PIN: &str = concatcp!(RUN_TASK_CHECKPOINT, "/pins/{pin_id}");
