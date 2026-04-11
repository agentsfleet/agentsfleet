//! Serve command argument parsing — extracted from serve.zig (M10_002).

const std = @import("std");

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
        if (std.mem.startsWith(u8, arg, "--port=")) {
            const port_raw = arg["--port=".len..];
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
