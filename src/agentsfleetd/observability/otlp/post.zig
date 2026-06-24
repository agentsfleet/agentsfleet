//! Persistent HTTP client + basic-auth POST for the OTLP exporters.
//!
//! One `Client` is created per flush thread and reused across flushes (HTTP
//! keep-alive), replacing the per-flush `std.http.Client` init/deinit the three
//! exporters used to do. The client is flush-thread-confined — only that single
//! thread touches it — so it needs no locking.

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");

pub const Client = struct {
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
    /// base64(instance_id:api_key). `alloc` is a per-flush scratch allocator for
    /// the URL + response buffer. Non-2xx is surfaced as an error so the caller's
    /// catch-warn logs the rejected export.
    pub fn post(
        self: *Client,
        alloc: std.mem.Allocator,
        cfg: config.GrafanaOtlpConfig,
        path: []const u8,
        payload: []const u8,
    ) !void {
        const endpoint = std.mem.trimEnd(u8, cfg.endpoint, "/");
        const url = std.fmt.allocPrint(alloc, "{s}{s}", .{ endpoint, path }) catch return;

        // Basic auth: base64(instance_id:api_key)
        var auth_raw_buf: [512]u8 = undefined;
        const auth_raw = std.fmt.bufPrint(&auth_raw_buf, "{s}:{s}", .{ cfg.instance_id, cfg.api_key }) catch return;
        const AUTH_B64_BUF_BYTES = 1024;
        var b64_buf: [AUTH_B64_BUF_BYTES]u8 = undefined;
        const b64_len = std.base64.standard.Encoder.calcSize(auth_raw.len);
        if (b64_len > b64_buf.len) return;
        const b64 = b64_buf[0..b64_len];
        _ = std.base64.standard.Encoder.encode(b64, auth_raw);
        var auth_header_buf: [1100]u8 = undefined;
        const auth_header = std.fmt.bufPrint(&auth_header_buf, "Basic {s}", .{b64}) catch return;

        var resp_body: std.ArrayList(u8) = .empty;
        var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_body);

        const headers: [2]std.http.Header = .{
            .{ .name = "content-type", .value = "application/json" },
            .{ .name = "authorization", .value = auth_header },
        };

        const result = self.inner.fetch(.{
            .location = .{ .url = url },
            .method = .POST,
            .payload = payload,
            .extra_headers = &headers,
            .response_writer = &aw.writer,
        }) catch return;

        // Non-2xx is a rejected export, not a success — surface it so the
        // caller's existing catch-warn logs it.
        if (result.status != .ok and result.status != .no_content and result.status != .accepted) {
            return error.OtlpExportRejected;
        }
    }
};

test {
    _ = @import("post_test.zig");
}
