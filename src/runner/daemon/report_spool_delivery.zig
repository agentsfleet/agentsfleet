//! report_spool_delivery.zig — the thin adapter between a held report and the
//! control plane, and the one place an HTTP status becomes a spool verdict.
//!
//! Extracted from `ReportSpool.zig` as the integration concern (Module Split
//! Pattern: `foo_integration.zig`, a thin adapter). The spool itself never
//! imports the client, which is what lets every rule it holds be tested with no
//! network; this file is the only seam where the two meet.
//!
//! `classify` encodes the report endpoint's own documented contract
//! (`afd_api_runner/src/handler/runner/report.rs`), and reads as that prose does:
//!
//!   - **200** — recorded. The endpoint is idempotent on the lease id, so a
//!     repeat after a lost response returns the stored outcome, charges nothing
//!     and counts the run once. That is the ENTIRE reason a replay is safe.
//!   - **403** — the fence refused it: this holder was superseded and the
//!     handler wrote nothing. "Retrying cannot change that", so the entry is
//!     spent rather than argued with.
//!   - **401** — a rejected token is an operational failure, not a verdict on
//!     the result. The result is kept: a token can rotate, a finished run
//!     cannot be re-finished.
//!   - **400 / 413 / 422** — the body itself is the problem, so no retry can
//!     fix it. Quarantined for an operator instead of spun forever.
//!   - **everything else** — a fault on their side or the wire. Keep and retry.
//!
//! The default arm is `retry_later` on purpose: an unrecognised status is an
//! unknown, and the safe answer to an unknown is to keep a finished run, not to
//! throw it away.

/// Turn one HTTP status into the decision the spool acts on.
pub fn classify(status: u16) Verdict {
    return switch (status) {
        STATUS_OK => .acknowledged,
        STATUS_FORBIDDEN => .superseded,
        STATUS_BAD_REQUEST, STATUS_PAYLOAD_TOO_LARGE, STATUS_UNPROCESSABLE => .invalid,
        else => .retry_later,
    };
}

/// The live control plane, bound to one runner token, as a `Delivery`.
///
/// Holds borrowed references for the length of a drain and owns nothing: the
/// client, the allocator and the token all outlive it, because a drain runs
/// inside the boot that built them.
pub const Plane = struct {
    client: *Client,
    alloc: Allocator,
    runner_token: []const u8,
    deadline_ms: u31,

    /// The `Delivery` this plane presents to a spool.
    pub fn delivery(self: *Plane) ReportSpool.Delivery {
        return .{ .ctx = self, .postFn = &post };
    }

    fn post(ctx: *anyopaque, body: []const u8) Verdict {
        const self: *Plane = @ptrCast(@alignCast(ctx));
        const result = self.client.post(
            self.alloc,
            protocol.PATH_RUNNER_REPORTS,
            self.runner_token,
            body,
            self.deadline_ms,
        ) catch |err| {
            // The wire, not an answer. A finished run is never discarded over a
            // connection, so this is always worth another boot.
            log.warn("report_spool_replay_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
            return .retry_later;
        };
        defer self.alloc.free(result.body);
        const verdict = classify(result.status);
        log.info("report_spool_replay_answered", .{ .status = result.status, .verdict = @tagName(verdict) });
        return verdict;
    }
};

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const client_errors = @import("../engine/client_errors.zig");
const ReportSpool = @import("ReportSpool.zig");

const Allocator = std.mem.Allocator;
// The client file IS the type (`const LoopbackClient = @This();`), so the
// module is the struct — the same shape `lease_run` and `worker_pool` import.
const Client = @import("control_plane_client.zig");
const Verdict = ReportSpool.Verdict;
const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

const STATUS_OK: u16 = 200;
const STATUS_BAD_REQUEST: u16 = 400;
const STATUS_FORBIDDEN: u16 = 403;
const STATUS_PAYLOAD_TOO_LARGE: u16 = 413;
const STATUS_UNPROCESSABLE: u16 = 422;

test "classify follows the report endpoint's own contract" {
    // Recorded, including the idempotent repeat a replay relies on.
    try std.testing.expectEqual(Verdict.acknowledged, classify(200));
    // The fence refused it and wrote nothing; retrying cannot change that.
    try std.testing.expectEqual(Verdict.superseded, classify(403));
    // The body is the problem, so an operator looks rather than a loop spins.
    try std.testing.expectEqual(Verdict.invalid, classify(400));
    try std.testing.expectEqual(Verdict.invalid, classify(413));
    try std.testing.expectEqual(Verdict.invalid, classify(422));
    // A rejected token is operational: the result outlives the credential.
    try std.testing.expectEqual(Verdict.retry_later, classify(401));
    try std.testing.expectEqual(Verdict.retry_later, classify(429));
    try std.testing.expectEqual(Verdict.retry_later, classify(500));
    try std.testing.expectEqual(Verdict.retry_later, classify(503));
    // An unknown status keeps a finished run rather than throwing it away.
    try std.testing.expectEqual(Verdict.retry_later, classify(418));
    try std.testing.expectEqual(Verdict.retry_later, classify(0));
}
