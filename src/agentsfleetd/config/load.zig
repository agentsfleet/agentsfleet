const std = @import("std");
const builtin = @import("builtin");
const S_T = " \t";
const S_T_R_N = " \t\r\n";

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
const DOTENV_MAX_BYTES = 1024 * 1024;

/// Overlay `.env.local` (non-overriding) onto a clone of the process env and
/// return the merged map for the caller to thread + `deinit`; null when dotenv
/// loading is off (caller keeps the process `env_map`). Zig 0.16 made the
/// environment an immutable snapshot from `std.process.Init` — a dotenv value
/// reaches config only by being merged into the map we thread, not via `setenv`.
pub fn applyEnvSources(
    io: std.Io,
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
) !?std.process.Environ.Map {
    if (!shouldLoadDotEnvLocal(env_map)) return null;
    var merged = try env_map.clone(alloc);
    errdefer merged.deinit();
    try overlayDotEnvLocal(io, &merged, alloc);
    return merged;
}

fn shouldLoadDotEnvLocal(env_map: *const std.process.Environ.Map) bool {
    if (env_map.get(ENV_ZOMBIED_LOAD_DOTENV)) |raw| {
        const trimmed = std.mem.trim(u8, raw, S_T_R_N);
        if (std.ascii.eqlIgnoreCase(trimmed, VAL_TRUE) or std.mem.eql(u8, trimmed, VAL_ONE)) return true;
        if (std.ascii.eqlIgnoreCase(trimmed, VAL_FALSE) or std.mem.eql(u8, trimmed, VAL_ZERO)) return false;
    }
    if (env_map.get(ENV_ZOMBIED_ENV_MODE)) |raw| {
        const trimmed = std.mem.trim(u8, raw, S_T_R_N);
        return std.ascii.eqlIgnoreCase(trimmed, ENV_MODE_DEV);
    }
    return builtin.mode == .Debug;
}

fn overlayDotEnvLocal(io: std.Io, merged: *std.process.Environ.Map, alloc: std.mem.Allocator) !void {
    const content = std.Io.Dir.cwd().readFileAlloc(io, PATH_DOTENV_LOCAL, alloc, .limited(DOTENV_MAX_BYTES)) catch |err| switch (err) {
        error.FileNotFound => return,
        else => return err,
    };
    defer alloc.free(content);

    var lines = std.mem.splitScalar(u8, content, '\n');
    while (lines.next()) |line_raw| {
        const line = std.mem.trim(u8, line_raw, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;

        const eq_idx = std.mem.indexOfScalar(u8, line, '=') orelse return LoadError.InvalidDotenvLine;
        const key = std.mem.trim(u8, line[0..eq_idx], S_T);
        if (key.len == 0) return LoadError.EmptyDotenvKey;

        const value_raw = std.mem.trim(u8, line[eq_idx + 1 ..], S_T);
        const value = stripOptionalQuotes(value_raw);
        // Non-overriding: a real env var wins over `.env.local`.
        if (merged.get(key) == null) try merged.put(key, value);
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

test "stripOptionalQuotes handles quoted and raw values" {
    try std.testing.expectEqualStrings("abc", stripOptionalQuotes("\"abc\""));
    try std.testing.expectEqualStrings("xyz", stripOptionalQuotes("'xyz'"));
    try std.testing.expectEqualStrings("plain", stripOptionalQuotes("plain"));
}
