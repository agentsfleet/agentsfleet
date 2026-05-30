//! HTTP client the host daemon uses to drive the `/v1/runners/me/*` control
//! plane. It POSTs lease/heartbeat/report/activity to a `zombied` instance
//! using the same frozen `protocol` shapes the server speaks, so server and
//! client cannot drift. Enrollment is not a daemon concern (Option B): the
//! operator pre-mints the `zrn_` and the daemon authenticates with it directly.
//!
//! Uses the high-level `std.http.Client.fetch` (cross-platform; the manual
//! `open()`/`readVec()` path is Linux-broken under Zig 0.15). One client per
//! call — these verbs are infrequent relative to a stage execution.

const LoopbackClient = @This();

/// Base origin of the control plane, e.g. `http://127.0.0.1:8080` (no path).
base_url: []const u8,

pub const ClientError = error{ RequestFailed, BadStatus, MalformedResponse };

/// POST /v1/runners/me/leases → the next event + resolved policy, or no-work.
/// The whole tree (event envelope + secrets_map + budget) lives in the returned
/// arena; the caller deinits after executing and reporting.
pub fn lease(self: LoopbackClient, alloc: Allocator, runner_token: []const u8) !std.json.Parsed(protocol.LeaseResponse) {
    const res = try self.post(alloc, protocol.PATH_RUNNER_LEASES, runner_token, "");
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    return std.json.parseFromSlice(protocol.LeaseResponse, alloc, res.body, .{}) catch
        ClientError.MalformedResponse;
}

/// POST /v1/runners/me/heartbeats → signal liveness + receive fleet directives.
/// Request body is empty in S0 (capacity/version fields are a later workstream).
/// Returns the parsed HeartbeatResponse so the daemon can act on status==drain/stop.
pub fn heartbeat(self: LoopbackClient, alloc: Allocator, runner_token: []const u8) !protocol.HeartbeatResponse {
    const res = try self.post(alloc, protocol.PATH_RUNNER_HEARTBEATS, runner_token, "");
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    const parsed = std.json.parseFromSlice(protocol.HeartbeatResponse, alloc, res.body, .{}) catch
        return ClientError.MalformedResponse;
    defer parsed.deinit();
    return parsed.value;
}

/// POST /v1/runners/me/reports → finalize one execution. Body is `{ok:true}`;
/// only the 2xx status matters to the caller.
pub fn report(self: LoopbackClient, alloc: Allocator, runner_token: []const u8, req: protocol.ReportRequest) !void {
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, protocol.PATH_RUNNER_REPORTS, runner_token, payload);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
}

/// POST /v1/runners/me/leases/{lease_id}/activity → forward live-tail progress
/// frames. `lease_id` is a path param (the only runner verb that takes one).
/// Best-effort by contract (202, no ack): the durable record is `report`, so a
/// failed forward is swallowed and never disturbs execution — hence `void`, not
/// `!void`. Allocation/transport failures drop the frame silently.
pub fn activity(
    self: LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
    frames: []const activity_wire.ActivityFrame,
) void {
    const path = std.fmt.allocPrint(alloc, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_ACTIVITY_SUFFIX,
    }) catch return;
    defer alloc.free(path);
    const payload = std.json.Stringify.valueAlloc(alloc, activity_wire.ActivityRequest{ .frames = frames }, .{}) catch return;
    defer alloc.free(payload);
    const res = self.post(alloc, path, runner_token, payload) catch return;
    alloc.free(res.body); // 202 expected; status ignored (best-effort, no ack).
}

/// Outcome of a renewal attempt the caller can act on without re-parsing.
pub const RenewResult = union(enum) {
    /// 2xx — the authoritative new kill deadline (epoch ms). Retarget the child.
    renewed: i64,
    /// A definitive 4xx (lease lost / max-runtime / no-credits): stop renewing
    /// and kill the child. Carries the status for the caller's log; the specific
    /// `UZ-RUN-010/011/012` distinction is server-logged.
    terminal: u16,
};

/// POST /v1/runners/me/leases/{lease_id}/renew → extend the lease's kill
/// deadline while the child is actively executing. `lease_id` is a path param.
///
/// Fail-safe by design: a 2xx yields `renewed`; a definitive 4xx yields
/// `terminal` (the caller kills its child); a transport failure or 5xx returns
/// an error so the caller simply retries on the next tick — if renewal keeps
/// failing the lease just expires naturally and is reclaimed (never double-run).
pub fn renew(
    self: LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
) !RenewResult {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer alloc.free(path);
    const res = try self.post(alloc, path, runner_token, "");
    defer alloc.free(res.body);
    if (res.status >= 200 and res.status < 300) {
        const parsed = std.json.parseFromSlice(protocol.RenewResponse, alloc, res.body, .{}) catch
            return ClientError.MalformedResponse;
        defer parsed.deinit();
        return .{ .renewed = parsed.value.lease_expires_at };
    }
    if (res.status >= 400 and res.status < 500) return .{ .terminal = res.status };
    return ClientError.BadStatus; // 5xx → retryable; caller retries next tick.
}

const PostResult = struct { status: u16, body: []u8 };

/// One bearer-authed POST. Returns the status + response body (owned by `alloc`).
fn post(self: LoopbackClient, alloc: Allocator, path: []const u8, bearer: []const u8, payload: []const u8) !PostResult {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ self.base_url, path });
    defer alloc.free(url);
    const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{bearer});
    defer alloc.free(auth);

    // BUFFER GATE: ArrayList for response body — fetch appends as it streams;
    // .items is read once for the JSON parse.
    var body: std.ArrayList(u8) = .{};
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth },
    };
    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch return ClientError.RequestFailed;

    return .{ .status = @intFromEnum(result.status), .body = aw.toOwnedSlice() catch return ClientError.RequestFailed };
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const protocol = @import("contract").protocol;
const activity_wire = @import("contract").activity;
