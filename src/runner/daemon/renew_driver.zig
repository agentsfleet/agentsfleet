//! Drives lease renewal from the child-supervisor's read-loop ticks.
//!
//! Holds the control-plane client and the lease's current kill deadline; once
//! inside the renewal window it calls `/renew` and maps the result to a
//! supervisor `RenewDecision`. The supervisor stays HTTP-agnostic — it only
//! knows `onTick(now) → keep|extend|terminate`; all the renewal/window/HTTP
//! logic lives here.
//!
//! Fail-safe: a transient/5xx renewal failure returns `keep` so the next tick
//! retries — if renewal never succeeds the lease simply expires and is reclaimed
//! (never double-run). A definitive 4xx (lost / capped / no-credits) returns
//! `terminate` and the run ends.
//!
//! Generic over the control-plane `Client` so production binds the real
//! `LoopbackClient` and tests inject a scripted fake — deterministic, no HTTP
//! and no wall clock (ZIG_RULES §Deterministic testing: inject the client, the
//! decision `now_ms` is passed in). `Client` must expose
//! `renew(self: *Client, alloc, runner_token, lease_id, req, deadline_ms) !RenewResult`.

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const constants = @import("common");
const child_supervisor = @import("../child_supervisor.zig");
const pipe_proto = @import("../pipe_proto.zig");
const client_errors = @import("../engine/client_errors.zig");

const LeasePayload = contract.protocol.LeasePayload;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

/// A u32 token-split triple ready for the wire — the explicit carrier the renew
/// body AND the report splits both map from, so neither path borrows the other's
/// HTTP-body type as a value bag. `renewRequest()` projects it onto the renew
/// body; the report path reads the fields directly. A field that later belongs
/// in `RenewRequest` but not the report (a fencing token, a posture hint) lands
/// on `RenewRequest`, never here, so it can never be silently zeroed into a report.
pub const TokenSplits = struct {
    input_tokens: u32 = 0,
    cached_input_tokens: u32 = 0,
    output_tokens: u32 = 0,

    /// Project onto the renew HTTP body. The report path reads the three fields
    /// directly rather than through this projection.
    pub fn renewRequest(self: TokenSplits) contract.protocol.RenewRequest {
        return .{
            .input_tokens = self.input_tokens,
            .cached_input_tokens = self.cached_input_tokens,
            .output_tokens = self.output_tokens,
        };
    }
};

/// Single source of the u64-cumulative → u32-wire mapping (the renew body and
/// the report splits both flow through here). The wire width is server-frozen
/// at u32, so we **saturate** rather than wrap: a wrap would drop the high bits
/// and massively under-bill (cumulative counts only grow, so a wrap reads as a
/// huge backward jump). Saturation instead clamps at u32 max — a *bounded*
/// under-bill, reached only past ~4.29B cumulative tokens per field per lease,
/// and never the catastrophic wrap. Beyond the clamp the server's cursor diff
/// (`GREATEST(0, sent − cursor)`) bills the excess at zero; the legacy `tokens`
/// total stays u64, so it can exceed the split sum once a field saturates.
pub fn wireSplits(input: u64, cached: u64, output: u64) TokenSplits {
    return .{
        .input_tokens = std.math.lossyCast(u32, input),
        .cached_input_tokens = std.math.lossyCast(u32, cached),
        .output_tokens = std.math.lossyCast(u32, output),
    };
}

/// Map the supervisor's live cumulative snapshot onto the renew wire body.
pub fn renewRequestFrom(usage: pipe_proto.UsageSnapshot) contract.protocol.RenewRequest {
    return wireSplits(usage.input_tokens, usage.cached_input_tokens, usage.output_tokens).renewRequest();
}

/// Build the renewal-driver type bound to a control-plane client type `Client`.
pub fn RenewDriver(comptime Client: type) type {
    return struct {
        const Self = @This();

        alloc: std.mem.Allocator,
        cp: Client,
        runner_token: []const u8,
        lease_id: []const u8,
        deadline_ms: i64,
        /// Read/write bound for each `/renew` call (config-resolved; the
        /// relation to the renewal window is enforced at config load).
        renew_deadline_ms: u31,

        /// Build a driver seeded with the lease's initial kill deadline.
        pub fn init(alloc: std.mem.Allocator, cp: Client, runner_token: []const u8, lease: LeasePayload, renew_deadline_ms: u31) Self {
            return .{
                .alloc = alloc,
                .cp = cp,
                .runner_token = runner_token,
                .lease_id = lease.lease_id,
                .deadline_ms = lease.lease_expires_at,
                .renew_deadline_ms = renew_deadline_ms,
            };
        }

        /// Build the supervisor hook bound to this driver (production tick cadence).
        pub fn hook(self: *Self) child_supervisor.RenewHook {
            return .{ .ctx = self, .onTick = onTick, .tick_ms = constants.RENEWAL_TICK_MS };
        }

        fn onTick(ctx: *anyopaque, now_ms: i64, usage: pipe_proto.UsageSnapshot) child_supervisor.RenewDecision {
            const self: *Self = @ptrCast(@alignCast(ctx));
            return self.tick(now_ms, usage);
        }

        /// One renewal decision. Called by the supervisor hook on each tick /
        /// progress frame (and composable by callers that fan a tick out to
        /// other periodic work). Renew only inside the window, posting the live
        /// cumulative usage so every mid-run slice bills its token spend; map
        /// the renewal result to a decision, advancing the deadline on success.
        pub fn tick(self: *Self, now_ms: i64, usage: pipe_proto.UsageSnapshot) child_supervisor.RenewDecision {
            // Equivalent to `deadline_ms - now_ms > WINDOW` but overflow-safe: a
            // garbage/extreme deadline from the wire must not panic the tick loop.
            if (self.deadline_ms > now_ms +| constants.RENEWAL_WINDOW_MS) return .keep;
            const res = self.cp.renew(self.alloc, self.runner_token, self.lease_id, renewRequestFrom(usage), self.renew_deadline_ms) catch |err| {
                log.warn("renew_failed_retry", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = self.lease_id, .err = @errorName(err) });
                return .keep;
            };
            switch (res) {
                .renewed => |new_deadline| {
                    self.deadline_ms = new_deadline;
                    log.debug("lease_renewed", .{ .lease_id = self.lease_id, .lease_expires_at = new_deadline });
                    return .{ .extend = new_deadline };
                },
                .terminal => |status| {
                    log.warn("lease_renew_terminal", .{ .error_code = client_errors.ERR_EXEC_RENEWAL_TERMINATED, .lease_id = self.lease_id, .status = status });
                    return .terminate;
                },
            }
        }
    };
}
