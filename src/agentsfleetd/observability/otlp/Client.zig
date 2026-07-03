//! Persistent HTTP client + basic-auth POST for the OTLP exporters.
//!
//! One `Client` is created per flush thread and reused across flushes (HTTP
//! keep-alive), replacing the per-flush `std.http.Client` init/deinit the three
//! exporters used to do. The client is flush-thread-confined — only that single
//! thread touches it — so it needs no locking.

const Client = @This();

inner: std.http.Client,

/// Create a persistent client. The client's own connection-pool bookkeeping
/// uses the page allocator (it lives across flushes); the process-global
/// blocking io matches the flush thread's blocking one-shot POST loop (not an
/// async event loop).
pub fn init() Client {
    return .{ .inner = .{ .allocator = std.heap.page_allocator, .io = common.globalIo() } };
}

pub fn deinit(self: *Client) void {
    self.inner.deinit();
}

/// POST `payload` to `{cfg.endpoint}{path}` with Basic auth
/// base64(instance_id:api_key). `alloc` is a per-flush scratch allocator for the
/// URL + response buffer. Every failure — a URL/auth formatting overflow, a
/// transport error, or a non-2xx status — propagates so the caller's catch-warn
/// logs it; none is swallowed into a bare success (an OTLP outage stays visible).
pub fn post(
    self: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    payload: []const u8,
) !void {
    const endpoint = std.mem.trimEnd(u8, cfg.endpoint, "/");
    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, path });
    defer alloc.free(url);

    // Basic auth: base64(instance_id:api_key)
    var auth_raw_buf: [512]u8 = undefined;
    const auth_raw = try std.fmt.bufPrint(&auth_raw_buf, "{s}:{s}", .{ cfg.instance_id, cfg.api_key });
    const AUTH_B64_BUF_BYTES = 1024;
    var b64_buf: [AUTH_B64_BUF_BYTES]u8 = undefined;
    const b64_len = std.base64.standard.Encoder.calcSize(auth_raw.len);
    // Invariant: base64 of a ≤512-byte auth string is ≤684 bytes, so it always
    // fits b64_buf. Asserted (not silently returned) so a future buffer-size
    // change that breaks the relationship trips in debug rather than truncating.
    std.debug.assert(b64_len <= b64_buf.len);
    const b64 = b64_buf[0..b64_len];
    _ = std.base64.standard.Encoder.encode(b64, auth_raw);
    var auth_header_buf: [1100]u8 = undefined;
    const auth_header = try std.fmt.bufPrint(&auth_header_buf, "Basic {s}", .{b64});

    // Response body is read only for its status, then discarded — deinit on every
    // path (the caller's scratch allocator is not always an arena).
    var resp_body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_body);
    defer aw.deinit();

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth_header },
    };

    const result = try self.inner.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    });

    // Non-2xx is a rejected export, not a success — surface it so the
    // caller's existing catch-warn logs it.
    if (result.status != .ok and result.status != .no_content and result.status != .accepted) {
        return error.OtlpExportRejected;
    }
}

test {
    _ = @import("Client_test.zig");
}

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");
