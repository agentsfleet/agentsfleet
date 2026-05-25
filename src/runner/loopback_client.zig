//! HTTP client the flag-gated walking skeleton uses to drive the `/v1/runners`
//! control plane over loopback. It POSTs register/lease/report to a `zombied`
//! instance (`127.0.0.1:<port>` in S0) using the same frozen `protocol` shapes
//! the server speaks, so server and client cannot drift.
//!
//! Uses the high-level `std.http.Client.fetch` (cross-platform; the manual
//! `open()`/`readVec()` path is Linux-broken under Zig 0.15). One client per
//! call — register/lease/report are infrequent relative to a stage execution.

const LoopbackClient = @This();

/// Base origin of the control plane, e.g. `http://127.0.0.1:8080` (no path).
base_url: []const u8,
/// Operator/provisioner credential (`zmb_t_` api_key or Clerk JWT) that authorizes
/// `register`. The minted runner token authorizes every later call.
register_token: []const u8,

pub const ClientError = error{ RequestFailed, BadStatus, MalformedResponse };

/// POST /v1/runners → mint a runner token. Returns the token owned by `alloc`.
pub fn register(self: LoopbackClient, alloc: Allocator, req: protocol.RegisterRequest) ![]u8 {
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, protocol.PATH_RUNNERS, self.register_token, payload);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
    const parsed = std.json.parseFromSlice(protocol.RegisterResponse, alloc, res.body, .{}) catch
        return ClientError.MalformedResponse;
    defer parsed.deinit();
    return alloc.dupe(u8, parsed.value.runner_token);
}

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

/// POST /v1/runners/me/reports → finalize one execution. Body is `{ok:true}`;
/// only the 2xx status matters to the caller.
pub fn report(self: LoopbackClient, alloc: Allocator, runner_token: []const u8, req: protocol.ReportRequest) !void {
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    const res = try self.post(alloc, protocol.PATH_RUNNER_REPORTS, runner_token, payload);
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return ClientError.BadStatus;
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
const protocol = @import("protocol.zig");
