//! Approval-gate constants: Redis key prefixes, timeouts, decision verbs, and
//! activity event names.
//!
//! These lived in `errors/error_registry.zig` until the library-reads
//! workstream needed room for a new error namespace and found that file full —
//! of these. Not one of them is an error code. They are the runtime vocabulary
//! of the approval gate, so they belong beside the `approval_gate*` family that
//! reads and writes them, and the error registry goes back to holding errors.
//!
//! RULE UFS: every value here is spelled once. A Redis prefix or an event name
//! that appears as a literal at a call site is the same bug this module exists
//! to prevent — the sweeper, the resolver, and the webhook handler must agree on
//! the exact bytes or a pending gate becomes unreachable.

/// Default wait before an unanswered gate resolves by policy.
pub const GATE_DEFAULT_TIMEOUT_MS: u64 = 3_600_000;
/// Upper bound for a configured gate timeout — larger values clamp + warn.
pub const GATE_TIMEOUT_MS_MAX: u64 = 86_400_000;
pub const GATE_ANOMALY_KEY_PREFIX = "fleet:anomaly:";
pub const GATE_PENDING_KEY_PREFIX = "fleet:gate:pending:";
pub const GATE_RESPONSE_KEY_PREFIX = "fleet:gate:response:";
/// event_id → "action_id|deadline_ms" ref the async lease-path gate check reads.
pub const GATE_EVENT_REF_KEY_PREFIX = "fleet:gate:byevent:";
pub const GATE_PENDING_TTL_SECONDS: u32 = 7200;
pub const GATE_DECISION_APPROVE = "approve";
pub const GATE_DECISION_DENY = "deny";

/// `core.fleet_approval_gates.gate_kind` for a standing integration
/// authorization, raised at install from the bundle's required credentials.
///
/// It is the arm selector in `RESOLVE_GATE`: resolving a gate carrying this
/// kind also moves the matching `core.integration_grants` row, so the standing
/// answer cannot drift from the decision that authorized it. The service it
/// asks about travels in `evidence->>'service'`, which that statement reads.
pub const GATE_KIND_INTEGRATION_GRANT = "integration_grant";
/// The `evidence` key naming the service an integration-grant gate covers.
pub const GATE_EVIDENCE_SERVICE_KEY = "service";

/// `core.fleet_approval_gates.gate_kind` for the unconditional write-fleet
/// park: a fleet whose repository binding declares WRITE access parks
/// every first-encounter event under this kind. Deliberately NOT a gate rule —
/// rules ride `config_json` (PATCHable under the same `fleet:write` scope that
/// wakes the fleet) and `.auto_approve` is their no-match fallthrough.
pub const GATE_KIND_REPOSITORY_WRITE = "repository_write";
const std = @import("std");

/// One approval funds this many write-credential requests. Requests spend
/// before vault or provider access, including cached and failed mints.
pub const REPOSITORY_WRITE_SPEND_CEILING: i64 = 32;
/// The write-kind card's blast radius, derived from the same ceiling stored on
/// the gate row so the human and the enforcement path cannot drift.
pub const GATE_BLAST_RADIUS_REPOSITORY_WRITE: []const u8 = std.fmt.comptimePrint(
    "up to {d} write-credential requests, one branch, and one draft Pull Request in the bound repository",
    .{REPOSITORY_WRITE_SPEND_CEILING},
);

// Gate activity event types
pub const GATE_EVENT_REQUIRED = "gate_approval_required";
pub const GATE_EVENT_APPROVED = "gate_approved";
pub const GATE_EVENT_DENIED = "gate_denied";
pub const GATE_EVENT_TIMEOUT = "gate_timeout";
pub const GATE_EVENT_AUTO_KILL = "gate_auto_kill";
pub const GATE_EVENT_AUTO_APPROVE = "gate_auto_approve";
