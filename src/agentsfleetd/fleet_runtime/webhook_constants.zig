//! Webhook and Slack ingress constants: Redis idempotency prefixes, the dedup
//! window, the activity event name, ingress status verbs, and Slack's
//! signature-header vocabulary.
//!
//! These lived in `errors/error_registry.zig` until a new error family found
//! that file full — of these. Not one of them is an error code, and the
//! error-codes audit greps that file for code literals, so it is the one file
//! in the tree that cannot afford squatters. They belong beside the ingress
//! path that reads and writes them, exactly as the approval-gate vocabulary
//! moved to `approval_gate_constants.zig` before them.
//!
//! RULE UFS: every value here is spelled once. A Redis prefix or a status verb
//! written as a literal at a call site is the bug this module exists to
//! prevent — both webhook handlers, the Slack events receiver, the cron fire
//! queue, and their tests must agree byte-for-byte.

/// How long an idempotency slot survives. A sender retrying beyond this window
/// is indistinguishable from a new delivery, so the window has to outlast every
/// upstream's retry schedule.
pub const DEDUP_TTL_SECONDS: u32 = 86400;

/// Redis key prefix for webhook idempotency slots (both webhook handlers and
/// their tests import it).
pub const WEBHOOK_DEDUP_KEY_PREFIX = "webhook:dedup:";

/// Redis key prefix for the Slack events idempotency slot, keyed on
/// `(channel_fleet_id, event.ts)` — Slack retries deliver the same `event.ts`.
pub const SLACK_DEDUP_KEY_PREFIX = "slack:dedup:";

pub const STATUS_ACCEPTED = "accepted";
pub const STATUS_DUPLICATE = "duplicate";

/// Webhook 200-ignored reason for a paused/non-active fleet: sender retry
/// queues add no value for an intentionally paused fleet.
pub const IGNORED_REASON_AGENTSFLEET_PAUSED = "fleet_paused";

// ── Slack signature vocabulary ──────────────────────────────────────────────

pub const SLACK_SIG_VERSION = "v0";
pub const SLACK_SIG_HEADER = "x-slack-signature";
pub const SLACK_TS_HEADER = "x-slack-request-timestamp";
/// Slack rejects a replayed request older than five minutes; matching that
/// bound here is what makes a stale-timestamp refusal ours rather than theirs.
pub const SLACK_MAX_TS_DRIFT_SECONDS: i64 = 300;
