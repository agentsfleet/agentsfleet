//! control_plane_client_renew.zig — the pure, I/O-free classification half of
//! the `/renew` verb.
//!
//! Split out of `control_plane_client.zig` (RULE FLL — that file sits at the line
//! cap) and re-exported there, so callers keep using `client.classifyRenew(...)`
//! and `client.RenewResult` unchanged. Mirrors the `control_plane_client_mint.zig`
//! split. The I/O half (`cp.renew`) stays on the parent, which owns the
//! connection pool and the deadline watchdog.
//!
//! Everything here is a pure function of `(status, body)`, so the whole
//! status→outcome mapping is unit-testable without a live server.

const std = @import("std");
const Allocator = std.mem.Allocator;
const types = @import("../engine/types.zig");
const client_errors = @import("../engine/client_errors.zig");
const LoopbackClient = @import("control_plane_client.zig");

const ClientError = LoopbackClient.ClientError;
const protocol = @import("contract").protocol;

/// A definitive `/renew` rejection: the status for the caller's log, plus the
/// `FailureClass` the run must be reported under. The reason is read off the
/// refusal body's `error_code` so a fleet's own budget stop (`UZ-RUN-015`) is
/// distinguishable in the durable `failure_label` from a platform/billing stop.
pub const TerminalRenew = struct {
    status: u16,
    reason: types.FailureClass = .renewal_terminate,
};

/// Outcome of a renewal attempt the caller can act on without re-parsing.
pub const RenewResult = union(enum) {
    /// 2xx — the authoritative new kill deadline (epoch ms). Retarget the child.
    renewed: i64,
    /// A definitive 4xx (lease lost / max-runtime / no-credits / fleet budget):
    /// stop renewing and kill the child, reporting `reason`.
    terminal: TerminalRenew,
};

/// The one HTTP status a fleet-budget refusal is served with (payment_required).
/// Gating on it means a stray `UZ-RUN-015` in a 401/404/409 body can never be
/// mistaken for a budget breach — the control plane only pairs that code with a
/// 402 (`service_renew.zig`).
const BUDGET_REFUSAL_STATUS: u16 = 402;

/// Map a refusal `(status, body)` onto the class the run is reported under. Only
/// a 402 whose body carries `UZ-RUN-015` is a budget breach; every other
/// definitive rejection — and any body we cannot read — stays `renewal_terminate`,
/// which is what those stops have always meant. An unreadable body must never
/// invent a more specific cause than we actually observed.
fn terminalReason(alloc: Allocator, status: u16, body: []const u8) types.FailureClass {
    if (status != BUDGET_REFUSAL_STATUS) return .renewal_terminate;
    const parsed = std.json.parseFromSlice(
        struct { error_code: []const u8 = "" },
        alloc,
        body,
        .{ .ignore_unknown_fields = true },
    ) catch return .renewal_terminate;
    defer parsed.deinit();
    if (std.mem.eql(u8, parsed.value.error_code, client_errors.ERR_RUN_BUDGET_EXCEEDED)) return .budget_breach;
    return .renewal_terminate;
}

/// Map a `/renew` HTTP response (status + body) to a `RenewResult`. A 2xx parses
/// the new kill deadline; a definitive 4xx yields `.terminal` carrying the class
/// read off its `error_code`; every other status (other 4xx, 5xx) is `BadStatus`
/// so the caller retries on the next tick.
pub fn classifyRenew(alloc: Allocator, status: u16, body: []const u8) ClientError!RenewResult {
    if (status >= 200 and status < 300) {
        const parsed = std.json.parseFromSlice(protocol.RenewResponse, alloc, body, .{}) catch
            return ClientError.MalformedResponse;
        defer parsed.deinit();
        return .{ .renewed = parsed.value.lease_expires_at };
    }
    if (isTerminalRenewStatus(status)) return .{ .terminal = .{ .status = status, .reason = terminalReason(alloc, status, body) } };
    return ClientError.BadStatus; // other 4xx (400/429/…) + 5xx → retryable; caller retries next tick.
}

/// Definitive `/renew` rejections the runner must NOT retry (kill the child):
/// 401 invalid/revoked token (UZ-RUN-001), 402 credit exhausted (UZ-RUN-012) or
/// fleet budget exhausted (UZ-RUN-015) — the body's `error_code` separates them,
/// 404 lease not found (UZ-RUN-006), 409 lease lost / max-runtime (UZ-RUN-010/011).
/// Any other 4xx (400 body, 429 rate-limit, …) is retryable like a 5xx — a
/// transient/non-terminal status must never kill a healthy in-flight run.
pub fn isTerminalRenewStatus(status: u16) bool {
    return status == 401 or status == 402 or status == 404 or status == 409;
}
