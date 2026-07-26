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

// Gate activity event types
pub const GATE_EVENT_REQUIRED = "gate_approval_required";
pub const GATE_EVENT_APPROVED = "gate_approved";
pub const GATE_EVENT_DENIED = "gate_denied";
pub const GATE_EVENT_TIMEOUT = "gate_timeout";
pub const GATE_EVENT_AUTO_KILL = "gate_auto_kill";
pub const GATE_EVENT_AUTO_APPROVE = "gate_auto_approve";
