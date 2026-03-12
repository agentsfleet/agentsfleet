//! Canonical trace context model for distributed tracing.
//! Follows W3C Trace Context (traceparent header) for interop.
//! Format: 00-<trace_id>-<span_id>-01
//!   trace_id: 16 bytes (32 hex chars)
//!   span_id:  8 bytes (16 hex chars)

const std = @import("std");

pub const TRACE_ID_HEX_LEN = 32;
pub const SPAN_ID_HEX_LEN = 16;

pub const TraceContext = struct {
    trace_id: [TRACE_ID_HEX_LEN]u8,
    span_id: [SPAN_ID_HEX_LEN]u8,
    parent_span_id: ?[SPAN_ID_HEX_LEN]u8 = null,

    /// Generate a new root trace context (no parent).
    pub fn generate() TraceContext {
        return .{
            .trace_id = randomHex(TRACE_ID_HEX_LEN),
            .span_id = randomHex(SPAN_ID_HEX_LEN),
        };
    }

    /// Create a child span under the same trace.
    pub fn child(self: *const TraceContext) TraceContext {
        return .{
            .trace_id = self.trace_id,
            .span_id = randomHex(SPAN_ID_HEX_LEN),
            .parent_span_id = self.span_id,
        };
    }

    pub fn traceIdSlice(self: *const TraceContext) []const u8 {
        return &self.trace_id;
    }

    pub fn spanIdSlice(self: *const TraceContext) []const u8 {
        return &self.span_id;
    }

    pub fn parentSpanIdSlice(self: *const TraceContext) []const u8 {
        if (self.parent_span_id) |*pid| return pid;
        return "";
    }

    /// Render as W3C traceparent header value: 00-{trace_id}-{span_id}-01
    pub fn toW3CHeader(self: *const TraceContext, buf: *[55]u8) []const u8 {
        @memcpy(buf[0..3], "00-");
        @memcpy(buf[3..35], &self.trace_id);
        buf[35] = '-';
        @memcpy(buf[36..52], &self.span_id);
        @memcpy(buf[52..55], "-01");
        return buf[0..55];
    }

    /// Parse a W3C traceparent header: 00-{32 hex}-{16 hex}-{2 hex}
    /// Returns null if the format is invalid.
    pub fn fromW3CHeader(header: []const u8) ?TraceContext {
        if (header.len < 55) return null;
        if (header[0] != '0' or header[1] != '0' or header[2] != '-') return null;
        if (header[35] != '-') return null;
        if (header[52] != '-') return null;

        const trace_hex = header[3..35];
        const span_hex = header[36..52];

        if (!isValidHex(trace_hex) or !isValidHex(span_hex)) return null;

        var ctx = TraceContext{
            .trace_id = undefined,
            .span_id = undefined,
        };
        @memcpy(&ctx.trace_id, trace_hex);
        @memcpy(&ctx.span_id, span_hex);
        return ctx;
    }
};

fn randomHex(comptime len: usize) [len]u8 {
    var raw: [len / 2]u8 = undefined;
    std.crypto.random.bytes(&raw);
    return std.fmt.bytesToHex(raw, .lower);
}

fn isValidHex(s: []const u8) bool {
    for (s) |c| {
        if (!std.ascii.isHex(c)) return false;
    }
    return true;
}

// --- Tests ---

test "generate creates valid trace context with unique IDs" {
    const ctx = TraceContext.generate();
    try std.testing.expect(isValidHex(&ctx.trace_id));
    try std.testing.expect(isValidHex(&ctx.span_id));
    try std.testing.expect(ctx.parent_span_id == null);
}

test "child preserves trace_id and sets parent_span_id" {
    const parent = TraceContext.generate();
    const kid = parent.child();

    try std.testing.expectEqualSlices(u8, &parent.trace_id, &kid.trace_id);
    try std.testing.expect(!std.mem.eql(u8, &parent.span_id, &kid.span_id));
    try std.testing.expectEqualSlices(u8, &parent.span_id, &kid.parent_span_id.?);
}

test "W3C traceparent round-trip" {
    const ctx = TraceContext.generate();
    var buf: [55]u8 = undefined;
    const header = ctx.toW3CHeader(&buf);

    try std.testing.expectEqual(@as(usize, 55), header.len);
    try std.testing.expectEqualSlices(u8, "00-", header[0..3]);
    try std.testing.expectEqualSlices(u8, "-01", header[52..55]);

    const parsed = TraceContext.fromW3CHeader(header) orelse
        return error.TestUnexpectedResult;

    try std.testing.expectEqualSlices(u8, &ctx.trace_id, &parsed.trace_id);
    try std.testing.expectEqualSlices(u8, &ctx.span_id, &parsed.span_id);
}

test "fromW3CHeader rejects malformed input" {
    try std.testing.expect(TraceContext.fromW3CHeader("") == null);
    try std.testing.expect(TraceContext.fromW3CHeader("not-a-traceparent-header-value") == null);
    try std.testing.expect(TraceContext.fromW3CHeader("01-00000000000000000000000000000000-0000000000000000-01") == null);
}

test "fromW3CHeader parses known-good value" {
    const parsed = TraceContext.fromW3CHeader("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01") orelse
        return error.TestUnexpectedResult;

    try std.testing.expectEqualSlices(u8, "4bf92f3577b34da6a3ce929d0e0e4736", &parsed.trace_id);
    try std.testing.expectEqualSlices(u8, "00f067aa0ba902b7", &parsed.span_id);
}

test "parentSpanIdSlice returns empty for root and span id for child" {
    const root = TraceContext.generate();
    try std.testing.expectEqual(@as(usize, 0), root.parentSpanIdSlice().len);

    const kid = root.child();
    try std.testing.expectEqualSlices(u8, &root.span_id, kid.parentSpanIdSlice());
}
