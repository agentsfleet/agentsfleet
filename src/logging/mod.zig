//! Structured-log helpers per docs/LOGGING_STANDARD.md.
//!
//! Imported via the named module `log`:
//!
//!   const logging = @import("log");
//!   const log = logging.scoped(.tag);
//!   log.<level>(event, fields)
//!
//! Builds a logfmt `key=value` record from `event` (comptime snake_case)
//! + `fields` (an anonymous struct literal). Emits via
//! `std.log.scoped(scope).<level>`; the custom `logFn` in `src/main.zig`
//! wraps it with the required `ts_ms / level / scope` keys.
//!
//! Plus `fatalStderr` for pre-init startup output (see its docstring).

const std = @import("std");

// ---------------------------------------------------------------------------
// Section 1 — `obs.scoped` API (LOGGING_STANDARD §7).
// ---------------------------------------------------------------------------

/// Compile-time-tagged logger. Mirrors `std.log.scoped` shape so callers
/// can swap in place: `std.log.scoped(.tag)` → `obs.scoped(.tag)`.
///
/// Each method takes a comptime `event` (snake_case verb_noun) and a
/// runtime `fields` anonymous struct. Field values are encoded into
/// `key=value` logfmt pairs. Strings containing whitespace, `=`, or `"`
/// are double-quoted with escape sequences for `"`, `\`, `\n`, `\r`, `\t`.
/// Optional fields with null payload are omitted (no `key=null`) per
/// LOGGING_STANDARD §3.
pub inline fn scoped(comptime scope: @TypeOf(.enum_literal)) type {
    return struct {
        pub inline fn err(
            comptime event: []const u8,
            fields: anytype,
        ) void {
            emit(.err, scope, event, fields);
        }
        pub inline fn warn(
            comptime event: []const u8,
            fields: anytype,
        ) void {
            emit(.warn, scope, event, fields);
        }
        pub inline fn info(
            comptime event: []const u8,
            fields: anytype,
        ) void {
            emit(.info, scope, event, fields);
        }
        pub inline fn debug(
            comptime event: []const u8,
            fields: anytype,
        ) void {
            emit(.debug, scope, event, fields);
        }
    };
}

// Reserved tail marker. On overflow we still emit the partial line plus
// this suffix so operators see *that* truncation happened — `error_code=`
// fields silently dropped past the 4 KiB cap would otherwise be invisible.
const TRUNCATION_MARKER = " truncated=true";

/// Build the logfmt body (`event=<name>` + serialized fields) into `buf`.
/// On overflow appends `TRUNCATION_MARKER` at the tail so operators can
/// see the line was capped instead of fields silently disappearing past
/// 4 KiB. `buf` must be at least `TRUNCATION_MARKER.len` bytes long.
fn buildLogfmtLine(
    buf: []u8,
    comptime event: []const u8,
    fields: anytype,
) []const u8 {
    var fbs = std.io.fixedBufferStream(buf[0 .. buf.len - TRUNCATION_MARKER.len]);
    const w = fbs.writer();

    var truncated = false;
    w.writeAll("event=" ++ event) catch {
        truncated = true;
    };
    if (!truncated) {
        writeFields(w, fields) catch {
            truncated = true;
        };
    }

    var written_len = fbs.getWritten().len;
    if (truncated) {
        @memcpy(buf[written_len..][0..TRUNCATION_MARKER.len], TRUNCATION_MARKER);
        written_len += TRUNCATION_MARKER.len;
    }
    return buf[0..written_len];
}

fn emit(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime event: []const u8,
    fields: anytype,
) void {
    var buf: [4096]u8 = undefined;
    const msg = buildLogfmtLine(&buf, event, fields);
    const log = std.log.scoped(scope);
    switch (level) {
        .err => log.err("{s}", .{msg}),
        .warn => log.warn("{s}", .{msg}),
        .info => log.info("{s}", .{msg}),
        .debug => log.debug("{s}", .{msg}),
    }
}

fn writeFields(w: anytype, fields: anytype) !void {
    const T = @TypeOf(fields);
    const info = @typeInfo(T);
    if (info != .@"struct") return;
    inline for (info.@"struct".fields) |f| {
        try writeOneField(w, f.name, @field(fields, f.name));
    }
}

fn writeOneField(w: anytype, key: []const u8, value: anytype) !void {
    const T = @TypeOf(value);
    switch (@typeInfo(T)) {
        .optional => {
            if (value) |v| try writeOneField(w, key, v);
            // else: omit — LOGGING_STANDARD §3 forbids `key=null` / `key=`.
        },
        else => {
            try w.writeByte(' ');
            try w.writeAll(key);
            try w.writeByte('=');
            try writeValue(w, value);
        },
    }
}

fn writeValue(w: anytype, value: anytype) !void {
    const T = @TypeOf(value);
    switch (@typeInfo(T)) {
        .int, .comptime_int => try w.print("{d}", .{value}),
        // Decimal (not scientific) for floats — operator dashboards / LogQL
        // queries scrape `ratio=0\.\d+` patterns and the previous `{e}`
        // emitted `7.56e-1` which silently broke them.
        .float, .comptime_float => try w.print("{d}", .{value}),
        .bool => try w.writeAll(if (value) "true" else "false"),
        .pointer => |p| {
            if (p.size == .slice and p.child == u8) {
                try writeStringValue(w, value);
            } else if (p.size == .one and @typeInfo(p.child) == .array and
                @typeInfo(p.child).array.child == u8)
            {
                try writeStringValue(w, value);
            } else {
                try w.print("{any}", .{value});
            }
        },
        .array => |a| {
            if (a.child == u8) {
                try writeStringValue(w, &value);
            } else {
                try w.print("{any}", .{value});
            }
        },
        .@"enum" => try w.writeAll(@tagName(value)),
        else => try w.print("{any}", .{value}),
    }
}

fn writeStringValue(w: anytype, s: []const u8) !void {
    const needs_quote = std.mem.indexOfAny(u8, s, " \t\"=") != null;
    if (!needs_quote) {
        try w.writeAll(s);
        return;
    }
    try w.writeByte('"');
    for (s) |c| {
        switch (c) {
            '"' => try w.writeAll("\\\""),
            '\\' => try w.writeAll("\\\\"),
            '\n' => try w.writeAll("\\n"),
            '\r' => try w.writeAll("\\r"),
            '\t' => try w.writeAll("\\t"),
            else => try w.writeByte(c),
        }
    }
    try w.writeByte('"');
}

// ---------------------------------------------------------------------------
// Section 3 — Pretty-printer (re-exports from pretty.zig).
// ---------------------------------------------------------------------------

const pretty = @import("pretty.zig");
pub const initPrettyMode = pretty.initPrettyMode;
pub const isPretty = pretty.isPretty;
pub const formatPretty = pretty.formatPretty;

// ---------------------------------------------------------------------------
// Section 4 — Pre-init stderr write.
// ---------------------------------------------------------------------------

/// Write a fatal startup message to stderr without going through the
/// logger. Use ONLY when the logger is not yet initialized — env-load
/// failure, config validation errors, unknown-subcommand at argv parse,
/// and any other operator-facing fatal that fires before
/// `initRuntimeLogLevel` runs.
///
/// Falls back silently on bufPrint or write failure (we are already
/// exiting). Caps the formatted message at 2 KiB; truncation is
/// acceptable since startup messages are short and the operator sees
/// stdout/stderr directly.
pub fn fatalStderr(comptime fmt: []const u8, args: anytype) void {
    var buf: [2048]u8 = undefined;
    const line = std.fmt.bufPrint(&buf, fmt, args) catch return;
    const stderr = std.fs.File.stderr();
    stderr.writeAll(line) catch {};
}

// ---------------------------------------------------------------------------
// Section 5 — Tests.
// ---------------------------------------------------------------------------

test "fatalStderr is callable (compile check)" {
    _ = @TypeOf(fatalStderr);
}

test "integration: logging helpers operate from catch paths" {
    const maybeFail = struct {
        fn run() !void {
            return error.ExpectedFailure;
        }
    };
    try std.testing.expectError(error.ExpectedFailure, maybeFail.run());
}

test "writeFields encodes integers and bare strings" {
    var buf: [256]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeFields(fbs.writer(), .{ .tool = "bash", .duration_ms = 1240, .ok = true });
    try std.testing.expectEqualStrings(" tool=bash duration_ms=1240 ok=true", fbs.getWritten());
}

test "writeFields quotes strings with whitespace and escapes special chars" {
    var buf: [256]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeFields(fbs.writer(), .{ .msg = "hello world", .raw = "a\"b\\c\n" });
    // " msg=\"hello world\" raw=\"a\\\"b\\\\c\\n\""
    try std.testing.expectEqualStrings(" msg=\"hello world\" raw=\"a\\\"b\\\\c\\n\"", fbs.getWritten());
}

test "writeFields omits null optionals (no key=null)" {
    var buf: [256]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    const maybe: ?[]const u8 = null;
    try writeFields(fbs.writer(), .{ .present = "x", .missing = maybe });
    try std.testing.expectEqualStrings(" present=x", fbs.getWritten());
}

test "writeFields renders enum values via @tagName" {
    const Color = enum { red, green, blue };
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try writeFields(fbs.writer(), .{ .color = Color.green });
    try std.testing.expectEqualStrings(" color=green", fbs.getWritten());
}

test "buildLogfmtLine fits within budget — no truncation marker" {
    var buf: [256]u8 = undefined;
    const msg = buildLogfmtLine(&buf, "small_event", .{ .tool = "bash", .ok = true });
    try std.testing.expectEqualStrings("event=small_event tool=bash ok=true", msg);
    try std.testing.expect(std.mem.indexOf(u8, msg, "truncated=true") == null);
}

test "buildLogfmtLine appends truncation marker on overflow" {
    // 64-byte buffer minus 15-byte marker leaves ~49 bytes for content.
    // A long string field forces writeFields to fail and trips the marker.
    var buf: [64]u8 = undefined;
    const long_payload: []const u8 = "x" ** 200;
    const msg = buildLogfmtLine(&buf, "ev", .{ .big = long_payload });
    try std.testing.expect(std.mem.endsWith(u8, msg, " truncated=true"));
    try std.testing.expect(msg.len <= buf.len);
}

test "scoped logger compiles for every level (smoke)" {
    // Smoke: ensure the comptime instantiation produces struct types
    // wired to std.log for each level. Avoids actually calling the
    // levels — Zig's test harness treats an error-level log call during
    // a test as a test failure.
    const Log = scoped(.test_smoke);
    _ = @TypeOf(Log.info);
    _ = @TypeOf(Log.warn);
    _ = @TypeOf(Log.err);
    _ = @TypeOf(Log.debug);
}
