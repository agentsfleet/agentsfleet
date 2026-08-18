//! The operator plane for a runner: its `admin_state` vocabulary, and the
//! `PATCH /v1/fleets/runners/{id}` body and reply.
//!
//! Lives in the shared wire layer because both runtimes read these shapes: the
//! dashboard sends them and the control plane decodes them. Split out of
//! `protocol.zig` on the 350-line bound (RULE FLL) when the self-test arm
//! landed — that file sat exactly at the cap and only aliases these now.

const policy = @import("protocol_policy.zig");

/// `fleet.runners.admin_state` — operator intent, a typed enum, app-enforced (no
/// SQL CHECK, per RULE STS). `active` admits the runner plane; cordoned/draining/
/// drained/revoked all reject it (→ 401 UZ-RUN-009). Renamed from `status`. The
/// enum is the single source for the operator PATCH; the string consts below are
/// derived from it (RULE UFS) for the SQL insert + the active gate. Not a wire value.
pub const AdminState = enum { active, cordoned, draining, drained, revoked };
/// The only `admin_state` that admits a runner-plane call — derived from the enum
/// (RULE UFS). Used by register (insert) and the runnerBearer lookup (active gate).
pub const ADMIN_STATE_ACTIVE = @tagName(AdminState.active);

/// Platform-admin mutation actions for `PATCH /v1/fleets/runners/{id}`. These
/// are wire enum values, so std.json accepts/serializes the tag names verbatim.
///
/// `self_test` is the odd one out: the other three transition `admin_state`,
/// while this records an operator's ASK and leaves the state alone. It rides
/// the same arm anyway because it shares everything that matters at the
/// boundary — the platform-operator scope, the exactly-one-of body guard, and
/// the refusal on a revoked runner. A second endpoint would duplicate all three.
pub const RunnerAdminAction = enum { cordon, drain, revoke, self_test };

/// PATCH body: exactly one of `action` (admin-state transition or a self-test
/// request) or `assigned_policy` (policy re-assignment — reaches the host on
/// its next heartbeat, no host visit). Both-or-neither is a 400; the handler
/// enforces it.
pub const RunnerAdminPatchRequest = struct {
    action: ?RunnerAdminAction = null,
    assigned_policy: ?policy.AssignedPolicy = null,
};

pub const RunnerAdminPatchResponse = struct {
    id: []const u8,
    admin_state: AdminState,
    /// Present on the policy-update path: the assignment as stored.
    assigned_policy: ?policy.AssignedPolicy = null,
    /// Present on the self-test path: when the request was recorded, epoch ms.
    ///
    /// The reply is the REQUEST, never a verdict — the daemon picks the ask up
    /// on its next heartbeat, so a result cannot exist yet. Returning the
    /// recorded instant lets the page age the pending state ("asked 40s ago")
    /// instead of showing a spinner with no clock behind it.
    selftest_requested_at: ?i64 = null,
};
