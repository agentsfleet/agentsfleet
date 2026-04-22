//! Clerk Backend API client — narrow surface for writing back user metadata
//! during signup bootstrap. Currently only supports the metadata-merge
//! endpoint (`POST /v1/users/{user_id}/metadata`) because that is the
//! single use case we have: after `signup_bootstrap.bootstrapPersonalAccount`
//! creates a tenant row, we need the next session JWT to carry the new
//! `tenant_id` + default `role=operator`. Clerk merges the payload rather
//! than replacing, so we do not need a read-then-write.
//!
//! Fire-and-log: the webhook handler calls `patchUserMetadata` and swallows
//! the error. A Clerk outage or a missing CLERK_SECRET_KEY must not turn
//! signup into a 500 — the DB row is already provisioned; the writeback is
//! best-effort observable state that the operator can repair manually.

const std = @import("std");

const log = std.log.scoped(.clerk_backend);

pub const SECRET_ENV_VAR = "CLERK_SECRET_KEY";
pub const API_BASE = "https://api.clerk.com/v1";

/// Upper bound on the Clerk PATCH round-trip. Zig 0.15's
/// `std.http.Client.fetch` does not expose connect/read timeouts in
/// `FetchOptions`, so we wrap the fetch in a bounded worker thread. A
/// slow Clerk region would otherwise hang the caller on OS-default TCP
/// timeouts (~2 min connect, long read). 5s is generous for an EU→US
/// metadata PATCH and still bounded for httpz's request lifecycle.
const FETCH_TIMEOUT_NS: u64 = 5 * std.time.ns_per_s;

pub const PatchError = error{
    MissingSecret,
    ConnectFailed,
    RequestFailed,
    Unauthorized,
    NotFound,
    UnexpectedStatus,
    SerializationFailed,
    OutOfMemory,
};

/// Merge the given public-metadata fields into the Clerk user's
/// `public_metadata` object. All fields are optional; omit a field by
/// passing null. Returns on 2xx; maps 401/404/5xx to the matching
/// `PatchError`.
///
/// Payload shape: `{"public_metadata": { ... }}` — Clerk's
/// `POST /v1/users/{id}/metadata` endpoint deep-merges into the existing
/// metadata, so sibling keys unknown to us (e.g. fields set by a future
/// admin dashboard) survive.
pub fn patchUserPublicMetadata(
    alloc: std.mem.Allocator,
    user_id: []const u8,
    tenant_id: ?[]const u8,
    role: ?[]const u8,
) PatchError!void {
    const secret = std.process.getEnvVarOwned(alloc, SECRET_ENV_VAR) catch |err| switch (err) {
        error.EnvironmentVariableNotFound => return PatchError.MissingSecret,
        error.OutOfMemory => return PatchError.OutOfMemory,
        else => return PatchError.RequestFailed,
    };
    defer alloc.free(secret);
    if (std.mem.trim(u8, secret, " \t\r\n").len == 0) return PatchError.MissingSecret;

    const payload = try renderMetadataPayload(alloc, tenant_id, role);
    defer alloc.free(payload);

    const url = try std.fmt.allocPrint(alloc, "{s}/users/{s}/metadata", .{ API_BASE, user_id });
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{secret});
    defer alloc.free(auth_header);

    return postMetadataMerge(alloc, url, auth_header, payload);
}

/// Split out so tests can drive the payload shape without an HTTP client.
pub fn renderMetadataPayload(
    alloc: std.mem.Allocator,
    tenant_id: ?[]const u8,
    role: ?[]const u8,
) PatchError![]u8 {
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(alloc);
    var w = buf.writer(alloc);

    w.writeAll("{\"public_metadata\":{") catch return PatchError.SerializationFailed;
    var first = true;
    if (tenant_id) |v| {
        writeJsonKeyValue(&w, &first, "tenant_id", v) catch return PatchError.SerializationFailed;
    }
    if (role) |v| {
        writeJsonKeyValue(&w, &first, "role", v) catch return PatchError.SerializationFailed;
    }
    w.writeAll("}}") catch return PatchError.SerializationFailed;
    return buf.toOwnedSlice(alloc) catch return PatchError.OutOfMemory;
}

fn writeJsonKeyValue(w: anytype, first: *bool, key: []const u8, value: []const u8) !void {
    if (!first.*) try w.writeAll(",");
    first.* = false;
    try w.writeAll("\"");
    try w.writeAll(key);
    try w.writeAll("\":\"");
    try writeJsonEscaped(w, value);
    try w.writeAll("\"");
}

/// Minimal JSON string-body escaper. Our values are either UUID v7
/// strings (`0195b4ba-…`, no special chars) or the literal role enum
/// labels from rbac.zig (`"operator"`, `"admin"`). Escaping `"`, `\`,
/// and ASCII control chars is sufficient — we never pass non-ASCII or
/// Unicode surrogate pairs through this path.
fn writeJsonEscaped(w: anytype, value: []const u8) !void {
    for (value) |c| switch (c) {
        '"' => try w.writeAll("\\\""),
        '\\' => try w.writeAll("\\\\"),
        '\n' => try w.writeAll("\\n"),
        '\r' => try w.writeAll("\\r"),
        '\t' => try w.writeAll("\\t"),
        // All ASCII control bytes outside the explicit \n/\r/\t branches,
        // plus DEL (0x7f). JSON permits bare DEL but downstream log
        // pipelines + operator consoles routinely choke on it, so we
        // escape defensively.
        0x00...0x08, 0x0B, 0x0C, 0x0E...0x1F, 0x7F => {
            var buf: [7]u8 = undefined;
            const hex = std.fmt.bufPrint(&buf, "\\u{x:0>4}", .{c}) catch unreachable;
            try w.writeAll(hex);
        },
        else => try w.writeAll(&[_]u8{c}),
    };
}

/// Shared state between the parent coroutine and the fetch worker.
/// Heap-allocated so both ends can observe its lifetime under the
/// mutex. Ownership: whichever side sees the other has disengaged
/// (parent abandons on timeout, worker signals done on completion)
/// frees the state. The first-out path carries no allocations, so
/// callers pay zero allocation overhead on the happy path's memory
/// footprint beyond the one struct + string dupes.
const FetchState = struct {
    alloc: std.mem.Allocator,
    mutex: std.Thread.Mutex = .{},
    cond: std.Thread.Condition = .{},
    done: bool = false,
    abandoned: bool = false,
    result: PatchError!void = {},
    url: []u8,
    auth_header: []u8,
    payload: []u8,
};

fn freeFetchState(state: *FetchState) void {
    const alloc = state.alloc;
    alloc.free(state.url);
    alloc.free(state.auth_header);
    alloc.free(state.payload);
    alloc.destroy(state);
}

fn fetchWorker(state: *FetchState) void {
    const r = runFetchBlocking(state.alloc, state.url, state.auth_header, state.payload);

    state.mutex.lock();
    state.result = r;
    state.done = true;
    const was_abandoned = state.abandoned;
    state.cond.broadcast();
    state.mutex.unlock();

    if (was_abandoned) freeFetchState(state);
}

fn postMetadataMerge(
    alloc: std.mem.Allocator,
    url: []const u8,
    auth_header: []const u8,
    payload: []const u8,
) PatchError!void {
    const state = alloc.create(FetchState) catch return PatchError.OutOfMemory;
    errdefer alloc.destroy(state);

    state.* = .{
        .alloc = alloc,
        .url = alloc.dupe(u8, url) catch return PatchError.OutOfMemory,
        .auth_header = alloc.dupe(u8, auth_header) catch return PatchError.OutOfMemory,
        .payload = alloc.dupe(u8, payload) catch return PatchError.OutOfMemory,
    };
    errdefer {
        alloc.free(state.url);
        alloc.free(state.auth_header);
        alloc.free(state.payload);
    }

    const thread = std.Thread.spawn(.{}, fetchWorker, .{state}) catch |err| {
        log.warn("clerk_backend.thread_spawn_fail err={s}", .{@errorName(err)});
        freeFetchState(state);
        return PatchError.RequestFailed;
    };
    thread.detach();

    state.mutex.lock();
    const deadline_ns = std.time.nanoTimestamp() + @as(i128, FETCH_TIMEOUT_NS);
    while (!state.done) {
        const now_ns = std.time.nanoTimestamp();
        if (now_ns >= deadline_ns) {
            state.abandoned = true;
            state.mutex.unlock();
            log.warn("clerk_backend.timeout url={s} after_ns={d}", .{ url, FETCH_TIMEOUT_NS });
            return PatchError.RequestFailed;
        }
        const remaining = @as(u64, @intCast(deadline_ns - now_ns));
        state.cond.timedWait(&state.mutex, remaining) catch {};
    }
    const result = state.result;
    state.mutex.unlock();
    freeFetchState(state);
    return result;
}

fn runFetchBlocking(
    alloc: std.mem.Allocator,
    url: []const u8,
    auth_header: []const u8,
    payload: []const u8,
) PatchError!void {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    var body: std.ArrayList(u8) = .{};
    defer body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth_header },
    };

    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch |err| return mapFetchError(err);

    return mapStatus(@intFromEnum(result.status), url);
}

/// Pure status→error mapping. Extracted so tests can exercise every
/// branch without standing up a mock HTTP server — a real mock would
/// need a TCP listener + response serialization and does not add
/// coverage over what this function is responsible for.
pub fn mapStatus(status: u16, url: []const u8) PatchError!void {
    if (status >= 200 and status < 300) return;
    if (status == 401 or status == 403) return PatchError.Unauthorized;
    if (status == 404) return PatchError.NotFound;
    log.warn("clerk_backend.unexpected_status status={d} url={s}", .{ status, url });
    return PatchError.UnexpectedStatus;
}

fn mapFetchError(err: anyerror) PatchError {
    return switch (err) {
        error.UnexpectedConnectFailure,
        error.ConnectionRefused,
        error.NetworkUnreachable,
        error.ConnectionTimedOut,
        error.HostUnreachable,
        error.PermissionDenied,
        error.TemporaryNameServerFailure,
        error.NameServerFailure,
        error.UnknownHostName,
        => PatchError.ConnectFailed,
        else => PatchError.RequestFailed,
    };
}

test {
    _ = @import("clerk_backend_test.zig");
}
