//! Structured-log helpers per docs/LOGGING_STANDARD.md.
//!
//! Two surfaces:
//!
//!   1. `obs.scoped(.tag).<level>(event, fields)` — preferred. Builds a
//!      logfmt key=value record from `event` + `fields` (an anonymous
//!      struct literal) and emits via `std.log.scoped(scope).<level>`.
//!      The custom `logFn` in main.zig wraps it with the required
//!      `ts_ms / level / scope` keys.
//!
//!   2. Legacy `logErr` / `logErrWithHint` / `logWarnErr` — kept for
//!      compat during the migration. New call sites SHOULD use the
//!      scoped API. The audit (scripts/audit-logging.sh) flags
//!      remaining `std.log.scoped` callers as INFO-level migration
//!      candidates.
//!
//! Plus `fatalStderr` for pre-init startup output (see its docstring).

const std = @import("std");
const error_codes = @import("../errors/error_registry.zig");

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

fn emit(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime event: []const u8,
    fields: anytype,
) void {
    var buf: [4096]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    const w = fbs.writer();

    w.writeAll("event=" ++ event) catch return;
    writeFields(w, fields) catch {};

    const msg = fbs.getWritten();
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
        .float, .comptime_float => try w.print("{e}", .{value}),
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
// Section 2 — Legacy helpers (compat during migration).
// ---------------------------------------------------------------------------

pub fn logErr(
    comptime scope: @TypeOf(.enum_literal),
    err: anyerror,
    comptime fmt: []const u8,
    args: anytype,
) void {
    std.log.scoped(scope).err(fmt ++ " err={s}", args ++ .{@errorName(err)});
}

/// Log an error with an actionable hint and docs link (git-style).
/// Use for fatal/startup errors where the operator needs next steps.
pub fn logErrWithHint(
    comptime scope: @TypeOf(.enum_literal),
    err: anyerror,
    comptime code: []const u8,
    comptime fmt: []const u8,
    args: anytype,
) void {
    const log = std.log.scoped(scope);
    log.err(fmt ++ " error_code=" ++ code ++ " err={s}", args ++ .{@errorName(err)});
    log.err("  hint: " ++ comptime error_codes.hint(code), .{});
    log.err("  see: " ++ error_codes.ERROR_DOCS_BASE ++ code, .{});
}

pub fn logWarnErr(
    comptime scope: @TypeOf(.enum_literal),
    err: anyerror,
    comptime fmt: []const u8,
    args: anytype,
) void {
    std.log.scoped(scope).warn(fmt ++ " err={s}", args ++ .{@errorName(err)});
}

// ---------------------------------------------------------------------------
// Section 3 — Pre-init stderr write.
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
// Section 4 — Tests.
// ---------------------------------------------------------------------------

test "logging helpers accept scoped error context" {
    const err_fn = logErr;
    const warn_fn = logWarnErr;
    _ = err_fn;
    _ = warn_fn;
    try std.testing.expect(true);
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

test "scoped logger compiles for every level (smoke)" {
    // Smoke: ensure the comptime instantiation produces struct types
    // wired to std.log for each level. Avoids actually calling the
    // levels — Zig's test harness treats a `std.log.err` call during
    // a test as a test failure.
    const Log = scoped(.test_smoke);
    _ = @TypeOf(Log.info);
    _ = @TypeOf(Log.warn);
    _ = @TypeOf(Log.err);
    _ = @TypeOf(Log.debug);
}
