// HTTP message types for the test harness — the fluent `Request` builder and
// the `Response` assertion helpers. Split out of `test_harness.zig` so each
// file stays under the file-length cap; the core harness re-exports both types
// so every consumer's import surface (`harness_mod.Request` / `.Response`)
// is unchanged.

const std = @import("std");
const harness_mod = @import("test_harness.zig");

pub const MAX_HEADERS = 16;
const RESPONSE_HEAD_BUFFER_SIZE = 16 * 1024;

/// Fluent request builder. Non-chaining — each method mutates and returns
/// by value. Keep on the caller's stack; `send` consumes.
pub const Request = struct {
    harness: *harness_mod.TestHarness,
    method: std.http.Method,
    path: []const u8,
    // SAFETY: test fixture; field is populated by the surrounding builder before any read.
    hdr_names: [MAX_HEADERS][]const u8 = undefined,
    // SAFETY: test fixture; field is populated by the surrounding builder before any read.
    hdr_values: [MAX_HEADERS][]const u8 = undefined,
    hdr_count: usize = 0,
    body: ?[]const u8 = null,
    bearer_owned: ?[]u8 = null, // allocated by bearer(); freed in send()'s defer
    /// null → std's default (follow up to 3). `.unhandled` returns a 3xx as-is,
    /// so a test can assert an OAuth callback's 302 without the client chasing
    /// Location to a dead `app_url`. Set via `redirectBehavior()`.
    redirect_behavior: ?std.http.Client.Request.RedirectBehavior = null,

    pub fn init(h: *harness_mod.TestHarness, method: std.http.Method, path: []const u8) Request {
        return .{ .harness = h, .method = method, .path = path };
    }

    pub fn header(self: Request, name: []const u8, value: []const u8) !Request {
        var r = self;
        if (r.hdr_count >= MAX_HEADERS) return error.TooManyHeaders;
        r.hdr_names[r.hdr_count] = name;
        r.hdr_values[r.hdr_count] = value;
        r.hdr_count += 1;
        return r;
    }

    pub fn bearer(self: Request, token: []const u8) !Request {
        std.debug.assert(self.bearer_owned == null); // double-bearer would leak the first allocation
        const val = try std.fmt.allocPrint(self.harness.alloc, "Bearer {s}", .{token});
        errdefer self.harness.alloc.free(val);
        var r = try self.header("authorization", val);
        r.bearer_owned = val;
        return r;
    }

    /// Adds `Content-Type: application/json` and sets body. Returns
    /// `error.TooManyHeaders` on slot overflow, matching `header()`'s contract —
    /// mixed assert/error is a footgun (Greptile #233 3106330937).
    pub fn json(self: Request, body: []const u8) !Request {
        var r = try self.header("content-type", "application/json");
        r.body = body;
        return r;
    }

    /// Raw body without content-type — caller sets it via `header`.
    pub fn rawBody(self: Request, body: []const u8) Request {
        var r = self;
        r.body = body;
        return r;
    }

    /// Bound/disable redirect following for this request. Default (unset) keeps
    /// std's follow-up-to-3; `.unhandled` returns a 3xx as-is — needed to assert
    /// an OAuth callback's 302 without the client chasing Location to a dead URL.
    pub fn redirectBehavior(self: Request, rb: std.http.Client.Request.RedirectBehavior) Request {
        var r = self;
        r.redirect_behavior = rb;
        return r;
    }

    pub fn send(self: Request) !Response {
        const alloc = self.harness.alloc;
        defer if (self.bearer_owned) |v| alloc.free(v);

        const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}{s}", .{ self.harness.port, self.path });
        defer alloc.free(url);

        var hdrs: [MAX_HEADERS]std.http.Header = undefined;
        var i: usize = 0;
        while (i < self.hdr_count) : (i += 1) {
            hdrs[i] = .{ .name = self.hdr_names[i], .value = self.hdr_values[i] };
        }

        var client: std.http.Client = .{ .allocator = alloc, .io = @import("common").globalIo() };
        defer client.deinit();
        var req = try client.request(self.method, try std.Uri.parse(url), .{
            .extra_headers = hdrs[0..self.hdr_count],
            .redirect_behavior = self.redirect_behavior orelse
                if (self.body == null) @enumFromInt(3) else .unhandled,
        });
        defer req.deinit();
        if (self.body) |body|
            try req.sendBodyComplete(@constCast(body))
        else
            try req.sendBodiless();

        var head_buffer: [RESPONSE_HEAD_BUFFER_SIZE]u8 = undefined;
        var response = try req.receiveHead(&head_buffer);
        const headers = try alloc.dupe(u8, response.head.bytes);
        errdefer alloc.free(headers);
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &buf);
        var transfer_buffer: [4096]u8 = undefined;
        _ = response.reader(&transfer_buffer).streamRemaining(&writer.writer) catch |err| switch (err) {
            error.ReadFailed => return response.bodyErr().?,
            else => |stream_err| return stream_err,
        };
        return .{
            .status = @intFromEnum(response.head.status),
            .body = try writer.toOwnedSlice(),
            .headers = headers,
            .alloc = alloc,
        };
    }
};

pub const Response = struct {
    status: u16,
    body: []u8,
    headers: ?[]u8 = null,
    alloc: std.mem.Allocator,

    pub fn deinit(self: Response) void {
        self.alloc.free(self.body);
        if (self.headers) |headers| self.alloc.free(headers);
    }

    pub fn expectStatus(self: Response, expected: std.http.Status) !void {
        const got = self.status;
        const want: u16 = @intFromEnum(expected);
        if (got != want) {
            std.log.warn("expectStatus: want {d}, got {d}; body={s}", .{ want, got, self.body });
            return error.UnexpectedStatus;
        }
    }

    /// Assert the RFC7807 problem+json error code matches. Tolerant of
    /// surrounding whitespace and field ordering — does a substring match
    /// on "\"error_code\":\"<code>\"" (the field name used in this repo's
    /// error envelope; see src/http/handlers/common.zig errorResponse).
    pub fn expectErrorCode(self: Response, code: []const u8) !void {
        const needle = try std.fmt.allocPrint(self.alloc, "\"error_code\":\"{s}\"", .{code});
        defer self.alloc.free(needle);
        if (std.mem.indexOf(u8, self.body, needle) == null) {
            std.log.warn("expectErrorCode: {s} not in body={s}", .{ code, self.body });
            return error.ErrorCodeMismatch;
        }
    }

    pub fn bodyContains(self: Response, needle: []const u8) bool {
        return std.mem.indexOf(u8, self.body, needle) != null;
    }

    pub fn header(self: Response, name: []const u8) ?[]const u8 {
        var it = std.http.HeaderIterator.init(self.headers orelse return null);
        while (it.next()) |header_value| {
            if (std.ascii.eqlIgnoreCase(header_value.name, name)) return header_value.value;
        }
        return null;
    }
};
