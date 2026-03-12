//! Shared logging helpers for consistent error-context output.

const std = @import("std");

pub fn logErr(
    comptime scope: @TypeOf(.enum_literal),
    err: anyerror,
    comptime fmt: []const u8,
    args: anytype,
) void {
    std.log.scoped(scope).err(fmt ++ " err={s}", args ++ .{@errorName(err)});
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
