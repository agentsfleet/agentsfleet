//! Serve command argument parsing — extracted from serve.zig.

const std = @import("std");

const S_PORT = "--port=";

/// Minimal `.next()`-yielding iterator over the threaded argv. Zig 0.16
/// removed `std.process.args()`; argv now arrives via `std.process.Init`.
pub const ArgvIter = struct {
    const Self = @This();

    argv: []const [:0]const u8,
    i: usize = 0,

    pub fn next(self: *Self) ?[:0]const u8 {
        if (self.i >= self.argv.len) return null;
        defer self.i += 1;
        return self.argv[self.i];
    }
};

/// Parse `agentsfleetd serve [--port N]` overrides from the raw argv (skips the
/// binary name + subcommand).
pub fn parseServeArgOverrides(argv: []const [:0]const u8) ServeArgError!?u16 {
    var it = ArgvIter{ .argv = argv };
    _ = it.next(); // binary name
    _ = it.next(); // subcommand
    return parseArgs(&it);
}

pub const ServeArgError = error{
    InvalidServeArgument,
    MissingPortValue,
    InvalidPortValue,
};

pub fn parseArgs(it: anytype) ServeArgError!?u16 {
    var override_port: ?u16 = null;
    while (it.next()) |arg| {
        if (std.mem.eql(u8, arg, "--port")) {
            const port_raw = it.next() orelse return ServeArgError.MissingPortValue;
            override_port = parsePortValue(port_raw) orelse return ServeArgError.InvalidPortValue;
            continue;
        }
        if (std.mem.startsWith(u8, arg, S_PORT)) {
            const port_raw = arg[S_PORT.len..];
            override_port = parsePortValue(port_raw) orelse return ServeArgError.InvalidPortValue;
            continue;
        }
        return ServeArgError.InvalidServeArgument;
    }
    return override_port;
}

pub fn parsePortValue(raw: []const u8) ?u16 {
    const parsed = std.fmt.parseInt(u16, raw, 10) catch return null;
    if (parsed == 0) return null;
    return parsed;
}
