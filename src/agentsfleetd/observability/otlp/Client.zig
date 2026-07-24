//! Persistent HTTP client + basic-auth POST for OpenTelemetry Protocol (OTLP)
//! exporters.
//!
//! One `Client` is created per flush thread and reused across flushes (HTTP
//! keep-alive), replacing the per-flush `std.http.Client` init/deinit the three
//! exporters used to do. The client is flush-thread-confined — only that single
//! thread touches it — so it needs no locking.

const Client = @This();

const std = @import("std");
const config = @import("config.zig");

const AUTH_B64_BUF_BYTES: usize = 1024;
const AUTH_HEADER_BUF_BYTES: usize = 1100;
const AUTH_RAW_BUF_BYTES: usize = 512;
const RESPONSE_BUF_BYTES: usize = 16 * 1024;
const S_AUTHORIZATION = "authorization";
const S_BASIC_S = "Basic {s}";
const S_CONTENT_TYPE = "content-type";
const S_JSON = "application/json";

io: std.Io,
inner: std.http.Client,

pub const ExportResult = union(enum) {
    accepted,
    partial_rejected: u64,
};

/// Create a persistent client. The client's own connection-pool bookkeeping
/// uses the page allocator (it lives across flushes); the process-global
/// blocking io matches the flush thread's blocking one-shot POST loop (not an
/// async event loop).
pub fn init(io: std.Io) Client {
    return .{
        .io = io,
        .inner = .{ .allocator = std.heap.page_allocator, .io = io },
    };
}

pub fn deinit(self: *Client) void {
    self.inner.deinit();
}

/// Post one payload and race it against the supplied absolute boot-clock
/// deadline. The losing task is canceled and joined before this client is reused.
pub fn post(
    self: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
    deadline_ns: i96,
) !ExportResult {
    if (deadlineReached(self.io, deadline_ns)) return error.OtlpExportTimedOut;

    const Selected = union(enum) {
        request: anyerror!ExportResult,
        timeout: std.Io.Cancelable!void,
    };
    var result_buf: [2]Selected = undefined;
    var select = std.Io.Select(Selected).init(self.io, &result_buf);
    try select.concurrent(.timeout, waitForDeadline, .{ self.io, deadline_ns });
    select.concurrent(.request, postTask, .{ self, alloc, cfg, path, payload, deadline_ns }) catch |err| {
        select.cancelDiscard();
        return err;
    };
    const selected = select.await() catch |err| {
        select.cancelDiscard();
        return err;
    };
    select.cancelDiscard();
    return switch (selected) {
        .request => |result| result,
        .timeout => |result| {
            result catch |err| switch (err) {
                error.Canceled => {},
            };
            return error.OtlpExportTimedOut;
        },
    };
}

fn postTask(
    self: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
    deadline_ns: i96,
) anyerror!ExportResult {
    return self.postOnce(alloc, cfg, path, payload, deadline_ns);
}

fn waitForDeadline(io: std.Io, deadline_ns: i96) std.Io.Cancelable!void {
    const deadline = std.Io.Timestamp.fromNanoseconds(deadline_ns).withClock(.boot);
    try deadline.wait(io);
}

fn deadlineReached(io: std.Io, deadline_ns: i96) bool {
    return std.Io.Clock.boot.now(io).toNanoseconds() >= deadline_ns;
}

fn postOnce(
    self: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
    deadline_ns: i96,
) !ExportResult {
    const endpoint = std.mem.trimEnd(u8, cfg.endpoint, "/");
    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, path });
    defer alloc.free(url);

    var auth_raw_buf: [AUTH_RAW_BUF_BYTES]u8 = undefined;
    var auth_b64_buf: [AUTH_B64_BUF_BYTES]u8 = undefined;
    var auth_header_buf: [AUTH_HEADER_BUF_BYTES]u8 = undefined;
    const auth_header = try formatAuthHeader(
        cfg,
        &auth_raw_buf,
        &auth_b64_buf,
        &auth_header_buf,
    );
    var response_buf: [RESPONSE_BUF_BYTES]u8 = undefined;
    var response_writer = std.Io.Writer.fixed(&response_buf);

    const headers: [2]std.http.Header = .{
        .{ .name = S_CONTENT_TYPE, .value = S_JSON },
        .{ .name = S_AUTHORIZATION, .value = auth_header },
    };

    if (deadlineReached(self.io, deadline_ns)) return error.OtlpExportTimedOut;
    const result = try self.inner.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &response_writer,
    });

    if (result.status != .ok and result.status != .no_content and result.status != .accepted) {
        return error.OtlpExportRejected;
    }
    return parseResponse(alloc, path, response_writer.buffered());
}

fn formatAuthHeader(
    cfg: config.GrafanaOtlpConfig,
    auth_raw_buf: *[AUTH_RAW_BUF_BYTES]u8,
    auth_b64_buf: *[AUTH_B64_BUF_BYTES]u8,
    auth_header_buf: *[AUTH_HEADER_BUF_BYTES]u8,
) ![]const u8 {
    const auth_raw = try std.fmt.bufPrint(auth_raw_buf, "{s}:{s}", .{ cfg.instance_id, cfg.api_key });
    const b64_len = std.base64.standard.Encoder.calcSize(auth_raw.len);
    std.debug.assert(b64_len <= auth_b64_buf.len);
    const b64 = auth_b64_buf[0..b64_len];
    _ = std.base64.standard.Encoder.encode(b64, auth_raw);
    return std.fmt.bufPrint(auth_header_buf, S_BASIC_S, .{b64});
}

fn parseResponse(
    alloc: std.mem.Allocator,
    path: []const u8,
    response: []const u8,
) !ExportResult {
    if (response.len == 0) return .accepted;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, response, .{}) catch return error.OtlpPartialResponseMalformed;
    defer parsed.deinit();
    const partial = switch (parsed.value) {
        .object => |object| object.get("partialSuccess") orelse return .accepted,
        else => return error.OtlpPartialResponseMalformed,
    };
    const rejected = switch (partial) {
        .object => |object| blk: {
            const key = if (std.mem.endsWith(u8, path, "/logs")) "rejectedLogRecords" else if (std.mem.endsWith(u8, path, "/traces")) "rejectedSpans" else "rejectedDataPoints";
            break :blk object.get(key) orelse return error.OtlpPartialResponseMalformed;
        },
        else => return error.OtlpPartialResponseMalformed,
    };
    return switch (rejected) {
        .integer => |value| if (value >= 0) .{ .partial_rejected = @intCast(value) } else error.OtlpPartialResponseMalformed,
        else => error.OtlpPartialResponseMalformed,
    };
}

test {
    _ = @import("Client_test.zig");
}
