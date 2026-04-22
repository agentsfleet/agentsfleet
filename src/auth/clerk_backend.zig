//! Clerk Backend API client — narrow surface for writing back user metadata
//! during signup bootstrap. Currently only supports the metadata-merge
//! endpoint (`POST /v1/users/{user_id}/metadata`) because that is the
//! single use case we have: after `signup_bootstrap.bootstrapPersonalAccount`
//! creates a tenant row, we need the next session JWT to carry the new
//! `tenant_id` + default `role=operator`. Clerk merges the payload rather
//! than replacing, so we do not need a read-then-write.
//!
//! Fire-and-forget: the webhook handler calls `patchUserPublicMetadata`
//! and ignores its return. So the HTTP call itself runs on a detached
//! worker — the webhook handler never blocks on Clerk. Success +
//! failure land in the log + a metric the worker increments before it
//! exits. A Clerk outage or a missing `CLERK_SECRET_KEY` must not turn
//! signup into a 500; the DB row is already provisioned and the
//! operator can repair publicMetadata via the Clerk Dashboard.

const std = @import("std");

const log = std.log.scoped(.clerk_backend);

pub const SECRET_ENV_VAR = "CLERK_SECRET_KEY";
pub const API_BASE = "https://api.clerk.com/v1";

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

/// Work item owned exclusively by the detached worker thread. The worker
/// runs the HTTP fetch, logs the outcome, increments a metric on failure,
/// and frees the item. The caller never touches this after spawn, so
/// there is no parent/worker coordination to get wrong.
///
/// **Allocator contract**: always `std.heap.c_allocator` (process-scoped).
/// A detached worker can outlive its caller's request arena; the
/// c_allocator never invalidates while the worker still holds pointers.
const FetchJob = struct {
    url: []u8,
    auth_header: []u8,
    payload: []u8,
};

fn freeFetchJob(job: *FetchJob) void {
    const alloc = std.heap.c_allocator;
    alloc.free(job.url);
    alloc.free(job.auth_header);
    alloc.free(job.payload);
    alloc.destroy(job);
}

fn fetchWorker(job: *FetchJob) void {
    defer freeFetchJob(job);
    runFetchBlocking(std.heap.c_allocator, job.url, job.auth_header, job.payload) catch |err| {
        log.warn("clerk_backend.fetch_failed err={s} url={s}", .{ @errorName(err), job.url });
    };
}

/// Spawn a detached worker that performs the HTTP fetch and exits. The
/// caller returns as soon as the job is queued — no waiting on Clerk.
/// This is the right shape for `writePublicMetadata` which already
/// swallows every error path; blocking the webhook handler on the
/// outbound RTT adds latency for no observable benefit.
///
/// Synchronous failures the caller can still observe:
///   - OutOfMemory (job allocation)
///   - Thread.spawn failure (mapped to RequestFailed)
///
/// Everything past Thread.spawn runs in the background and reports via
/// log + metric.
fn postMetadataMerge(
    _caller_alloc: std.mem.Allocator,
    url: []const u8,
    auth_header: []const u8,
    payload: []const u8,
) PatchError!void {
    _ = _caller_alloc;
    const stable = std.heap.c_allocator;

    const job = try prepareFetchJob(stable, url, auth_header, payload);
    errdefer freeFetchJob(job);

    const thread = std.Thread.spawn(.{}, fetchWorker, .{job}) catch |err| {
        log.warn("clerk_backend.thread_spawn_fail err={s}", .{@errorName(err)});
        return PatchError.RequestFailed;
    };
    thread.detach();
    // Worker owns `job` now — erdefer above is not invoked because
    // spawn succeeded, and no further error paths remain in this
    // function.
}

/// Build a fully-initialized `*FetchJob` or bubble up OutOfMemory. All
/// cleanup errdefers live here, never past the spawn boundary.
fn prepareFetchJob(
    stable: std.mem.Allocator,
    url: []const u8,
    auth_header: []const u8,
    payload: []const u8,
) PatchError!*FetchJob {
    const job = stable.create(FetchJob) catch return PatchError.OutOfMemory;
    errdefer stable.destroy(job);

    job.* = .{
        .url = stable.dupe(u8, url) catch return PatchError.OutOfMemory,
        .auth_header = undefined,
        .payload = undefined,
    };
    errdefer stable.free(job.url);

    job.auth_header = stable.dupe(u8, auth_header) catch return PatchError.OutOfMemory;
    errdefer stable.free(job.auth_header);

    job.payload = stable.dupe(u8, payload) catch return PatchError.OutOfMemory;
    return job;
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
