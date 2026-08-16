//! Pretty-printer for log records (LOGGING_STANDARD §9, dev only).
//!
//! Detection: `AGENTSFLEET_LOG_PRETTY=1|true` forces on, `=0|false` forces
//! off, otherwise auto-detects via `isatty(stderr)`. Result cached in
//! a module-level `var` at process init; never re-checked per call.

const std = @import("std");

const PrettyMode = enum { off, on };

var pretty_mode: PrettyMode = .off;

/// `pretty_raw` is the resolved `AGENTSFLEET_LOG_PRETTY` value (or null when unset);
/// `stderr_is_tty` is the caller's resolved TTY probe. Zig 0.16 made env a
/// threaded snapshot and removed `std.posix.isatty`, so the caller (main) reads
/// both via the threaded `io`/`env_map` and passes them here — keeping the
/// `log` module free of `common`/`io` dependencies. `1|true` forces on,
/// `0|false` forces off, otherwise the TTY probe decides.
pub fn initPrettyMode(pretty_raw: ?[]const u8, stderr_is_tty: bool) void {
    if (pretty_raw) |val| {
        if (std.mem.eql(u8, val, "1") or std.mem.eql(u8, val, "true")) {
            pretty_mode = .on;
            return;
        }
        if (std.mem.eql(u8, val, "0") or std.mem.eql(u8, val, "false")) {
            pretty_mode = .off;
            return;
        }
    }
    pretty_mode = if (stderr_is_tty) .on else .off;
}

pub inline fn isPretty() bool {
    return pretty_mode == .on;
}

const ansi = struct {
    const reset = "\x1b[0m";
    const dim = "\x1b[2m";
    const bold = "\x1b[1m";
    const red = "\x1b[31m";
    const yellow = "\x1b[33m";
    const cyan = "\x1b[36m";
    const grey = "\x1b[90m";
    const bright_white = "\x1b[97m";
};

/// Render a log record in Bun-style colored format. Same record contents
/// as logfmt; different sink. Returns a slice of the provided buffer.
pub fn formatPretty(
    buf: []u8,
    ts_ms: i64,
    level: std.log.Level,
    scope: []const u8,
    msg: []const u8,
) []const u8 {
    const level_color = switch (level) {
        .err => ansi.red,
        .warn => ansi.yellow,
        .info => ansi.cyan,
        .debug => ansi.grey,
    };
    const level_label: []const u8 = switch (level) {
        .err => "ERROR",
        .warn => "WARN ",
        .info => "INFO ",
        .debug => "DEBUG",
    };
    const day_seconds = @mod(@divFloor(ts_ms, 1000), 86400);
    const hours: u32 = @intCast(@divFloor(day_seconds, 3600));
    const minutes: u32 = @intCast(@divFloor(@mod(day_seconds, 3600), 60));
    const seconds: u32 = @intCast(@mod(day_seconds, 60));
    const ms: u32 = @intCast(@mod(ts_ms, 1000));

    return std.fmt.bufPrint(
        buf,
        "{s}{d:0>2}:{d:0>2}:{d:0>2}.{d:0>3}{s}  {s}{s}{s}  {s}{s:<18}{s}  {s}\n",
        .{
            ansi.grey,   hours,       minutes,    seconds,   ms,    ansi.reset,
            level_color, level_label, ansi.reset, ansi.cyan, scope, ansi.reset,
            msg,
        },
    ) catch buf[0..0];
}

test "formatPretty produces expected layout" {
    var buf: [512]u8 = undefined;
    // 14:32:18.123 UTC = 14*3600 + 32*60 + 18 = 52338 seconds + 123 ms
    const ts: i64 = 52338 * 1000 + 123;
    const out = formatPretty(&buf, ts, .info, "auth", "event=jwks_fetched keys=3");
    // Should contain HH:MM:SS.fff and INFO + scope + msg.
    try std.testing.expect(std.mem.indexOf(u8, out, "14:32:18.123") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "INFO") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "auth") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "event=jwks_fetched keys=3") != null);
}

test "isPretty defaults off before init" {
    pretty_mode = .off;
    try std.testing.expect(!isPretty());
}

test "formatPretty gives every level its own colour and label" {
    // Colour IS the interface here. An operator scanning a pretty log finds
    // errors by looking for red, so a level rendered in the wrong colour is a
    // missed incident, not a cosmetic bug. Only `.info` was pinned before, and
    // three of the four arms could have been swapped without a test noticing.
    const cases = .{
        .{ .level = std.log.Level.err, .colour = ansi.red, .label = "ERROR" },
        .{ .level = std.log.Level.warn, .colour = ansi.yellow, .label = "WARN " },
        .{ .level = std.log.Level.info, .colour = ansi.cyan, .label = "INFO " },
        .{ .level = std.log.Level.debug, .colour = ansi.grey, .label = "DEBUG" },
    };

    inline for (cases) |case| {
        var buf: [512]u8 = undefined;
        const out = formatPretty(&buf, 0, case.level, "probe", "event=x");
        try std.testing.expect(std.mem.indexOf(u8, out, case.label) != null);
        // The colour must sit immediately before its own label, not merely
        // appear somewhere in the line — the timestamp is grey and the scope is
        // cyan, so a loose search would pass for the wrong arm.
        const at = std.mem.indexOf(u8, out, case.label) orelse return error.LabelMissing;
        try std.testing.expect(at >= case.colour.len);
        try std.testing.expectEqualStrings(case.colour, out[at - case.colour.len .. at]);
    }
}

test "formatPretty returns empty rather than truncating into a buffer it cannot fill" {
    // The fallback arm. A record wider than the caller's buffer must yield
    // nothing at all: a half-written line carrying an unterminated escape
    // sequence would recolour the operator's whole terminal from that point on.
    var tiny: [8]u8 = undefined;
    const out = formatPretty(&tiny, 0, .err, "scope-that-does-not-fit", "event=far_too_long");
    try std.testing.expectEqual(@as(usize, 0), out.len);
}
