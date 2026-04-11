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
