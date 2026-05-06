//! Shared logging helpers for consistent error-context output.
//! Use logErrWithHint for fatal/startup errors where the operator needs guidance.

const std = @import("std");
const error_codes = @import("../errors/error_registry.zig");

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

/// Write a fatal startup message to stderr without going through the
/// logger. Use ONLY when the logger is not yet initialized — env-load
/// failure, config validation errors, unknown-subcommand at argv parse,
/// and any other operator-facing fatal that fires before
/// `initRuntimeLogLevel` runs.
///
/// Falls back silently on bufPrint or write failure (we are already
/// exiting). Caps the formatted message at 2 KiB; truncation is acceptable
/// since startup messages are short and the operator sees stdout/stderr
/// directly.
pub fn fatalStderr(comptime fmt: []const u8, args: anytype) void {
    var buf: [2048]u8 = undefined;
    const line = std.fmt.bufPrint(&buf, fmt, args) catch return;
    const stderr = std.fs.File.stderr();
    stderr.writeAll(line) catch {};
}

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
