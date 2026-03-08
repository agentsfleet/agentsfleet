const std = @import("std");
const builtin = @import("builtin");

const LoadError = error{
    InvalidDotenvLine,
    EmptyDotenvKey,
};

const PATH_DOTENV_LOCAL = ".env.local";
const ENV_ZOMBIED_LOAD_DOTENV = "ZOMBIED_LOAD_DOTENV";
const ENV_ZOMBIED_ENV_MODE = "ZOMBIED_ENV_MODE";
const ENV_MODE_DEV = "dev";
const VAL_TRUE = "true";
const VAL_FALSE = "false";
const VAL_ONE = "1";
const VAL_ZERO = "0";

pub fn applyEnvSources(alloc: std.mem.Allocator) !void {
    if (!shouldLoadDotEnvLocal(alloc)) return;
    try loadDotEnvLocalNonOverriding(alloc);
}

fn shouldLoadDotEnvLocal(alloc: std.mem.Allocator) bool {
    if (std.process.getEnvVarOwned(alloc, ENV_ZOMBIED_LOAD_DOTENV)) |raw| {
        defer alloc.free(raw);
        const trimmed = std.mem.trim(u8, raw, " \t\r\n");
        if (std.ascii.eqlIgnoreCase(trimmed, VAL_TRUE) or std.mem.eql(u8, trimmed, VAL_ONE)) return true;
        if (std.ascii.eqlIgnoreCase(trimmed, VAL_FALSE) or std.mem.eql(u8, trimmed, VAL_ZERO)) return false;
    } else |_| {}

    if (std.process.getEnvVarOwned(alloc, ENV_ZOMBIED_ENV_MODE)) |raw| {
        defer alloc.free(raw);
        const trimmed = std.mem.trim(u8, raw, " \t\r\n");
        return std.ascii.eqlIgnoreCase(trimmed, ENV_MODE_DEV);
    } else |_| {}

    return builtin.mode == .Debug;
}

fn loadDotEnvLocalNonOverriding(alloc: std.mem.Allocator) !void {
    const file = std.fs.cwd().openFile(PATH_DOTENV_LOCAL, .{}) catch |err| switch (err) {
        error.FileNotFound => return,
        else => return err,
    };
    defer file.close();

    const content = try file.readToEndAlloc(alloc, 1024 * 1024);
    defer alloc.free(content);

    var lines = std.mem.splitScalar(u8, content, '\n');
    while (lines.next()) |line_raw| {
        const line = std.mem.trim(u8, line_raw, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;

        const eq_idx = std.mem.indexOfScalar(u8, line, '=') orelse return LoadError.InvalidDotenvLine;
        const key = std.mem.trim(u8, line[0..eq_idx], " \t");
        if (key.len == 0) return LoadError.EmptyDotenvKey;

        const value_raw = std.mem.trim(u8, line[eq_idx + 1 ..], " \t");
        const value = stripOptionalQuotes(value_raw);
        try setIfMissingEnv(key, value);
    }
}

fn stripOptionalQuotes(raw: []const u8) []const u8 {
    if (raw.len >= 2) {
        const first = raw[0];
        const last = raw[raw.len - 1];
        if ((first == '"' and last == '"') or (first == '\'' and last == '\'')) {
            return raw[1 .. raw.len - 1];
        }
    }
    return raw;
}

fn setIfMissingEnv(key: []const u8, value: []const u8) !void {
    if (std.posix.getenv(key) != null) return;
    switch (builtin.os.tag) {
        .windows => {
            // NOTE: process-level env fallback for Windows can be added if needed.
            return;
        },
        else => {
            if (std.c.setenv(key.ptr, value.ptr, 0) != 0) return error.Unexpected;
        },
    }
}

test "stripOptionalQuotes handles quoted and raw values" {
    try std.testing.expectEqualStrings("abc", stripOptionalQuotes("\"abc\""));
    try std.testing.expectEqualStrings("xyz", stripOptionalQuotes("'xyz'"));
    try std.testing.expectEqualStrings("plain", stripOptionalQuotes("plain"));
}
