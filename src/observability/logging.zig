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
    logErr(.observability, error.TestError, "logErr smoke test boundary={s}", .{"unit"});
    logWarnErr(.observability, error.TestError, "logWarnErr smoke test boundary={s}", .{"unit"});
}

test "integration: logging helpers operate from catch paths" {
    const maybeFail = struct {
        fn run() !void {
            return error.ExpectedFailure;
        }
    };
    maybeFail.run() catch |err| {
        logWarnErr(.observability, err, "catch-path logging smoke test stage={s}", .{"integration"});
    };
}
