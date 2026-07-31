const std = @import("std");
const builtin = @import("builtin");
const S_T = " \t";
/// The one whitespace trim set env-value parsing uses (RULE UFS — consumed by
/// the dotenv gate, env_vars, and the runtime loader; no re-spelled copies).
pub const TRIM_SET = " \t\r\n";
const S_T_R_N = TRIM_SET;

const LoadError = error{
    InvalidDotenvLine,
    EmptyDotenvKey,
};

const PATH_DOTENV_LOCAL = ".env.local";
const ENV_AGENTSFLEETD_LOAD_DOTENV = "AGENTSFLEETD_LOAD_DOTENV";
const ENV_AGENTSFLEETD_ENV_MODE = "AGENTSFLEETD_ENV_MODE";
const ENV_MODE_DEV = "dev";
const VAL_TRUE = "true";
const VAL_FALSE = "false";
const VAL_ONE = "1";
const VAL_ZERO = "0";
const DOTENV_MAX_BYTES = 1024 * 1024;

/// Where a dotenv parse failed — populated on `InvalidDotenvLine` /
/// `EmptyDotenvKey` so the boot error can name the offending line
/// (1-based; 0 = no dotenv failure).
pub const DotenvDiagnostic = struct { line: usize = 0 };

/// Overlay `.env.local` (non-overriding) onto a clone of the process env and
/// return the merged map for the caller to thread + `deinit`; null when dotenv
/// loading is off (caller keeps the process `env_map`). Zig 0.16 made the
/// environment an immutable snapshot from `std.process.Init` — a dotenv value
/// reaches config only by being merged into the map we thread, not via `setenv`.
pub fn applyEnvSources(
    io: std.Io,
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    diag: *DotenvDiagnostic,
) !?std.process.Environ.Map {
    if (!shouldLoadDotEnvLocal(env_map)) return null;
    var merged = try env_map.clone(alloc);
    errdefer merged.deinit();
    try overlayDotEnvLocal(io, &merged, alloc, diag);
    return merged;
}

/// One env-boolean grammar for every boolean env var: trimmed, then
/// case-insensitive true/false or exact 1/0. Callers pick their own policy
/// for `.invalid` (permissive fallthrough vs strict boot error).
pub const EnvBool = enum { yes, no, invalid };

pub fn parseEnvBool(raw: []const u8) EnvBool {
    const trimmed = std.mem.trim(u8, raw, S_T_R_N);
    if (std.ascii.eqlIgnoreCase(trimmed, VAL_TRUE) or std.mem.eql(u8, trimmed, VAL_ONE)) return .yes;
    if (std.ascii.eqlIgnoreCase(trimmed, VAL_FALSE) or std.mem.eql(u8, trimmed, VAL_ZERO)) return .no;
    return .invalid;
}

fn shouldLoadDotEnvLocal(env_map: *const std.process.Environ.Map) bool {
    if (env_map.get(ENV_AGENTSFLEETD_LOAD_DOTENV)) |raw| {
        switch (parseEnvBool(raw)) {
            .yes => return true,
            .no => return false,
            .invalid => {},
        }
    }
    if (env_map.get(ENV_AGENTSFLEETD_ENV_MODE)) |raw| {
        const trimmed = std.mem.trim(u8, raw, S_T_R_N);
        return std.ascii.eqlIgnoreCase(trimmed, ENV_MODE_DEV);
    }
    return builtin.mode == .Debug;
}

fn overlayDotEnvLocal(io: std.Io, merged: *std.process.Environ.Map, alloc: std.mem.Allocator, diag: *DotenvDiagnostic) !void {
    const content = std.Io.Dir.cwd().readFileAlloc(io, PATH_DOTENV_LOCAL, alloc, .limited(DOTENV_MAX_BYTES)) catch |err| switch (err) {
        error.FileNotFound => return,
        else => return err,
    };
    defer alloc.free(content);
    return overlayDotEnvContent(merged, content, diag);
}

fn overlayDotEnvContent(merged: *std.process.Environ.Map, content: []const u8, diag: *DotenvDiagnostic) !void {
    var lines = std.mem.splitScalar(u8, content, '\n');
    var line_no: usize = 0;
    while (lines.next()) |line_raw| {
        line_no += 1;
        const line = std.mem.trim(u8, line_raw, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;

        const eq_idx = std.mem.indexOfScalar(u8, line, '=') orelse {
            diag.line = line_no;
            return LoadError.InvalidDotenvLine;
        };
        const key = std.mem.trim(u8, line[0..eq_idx], S_T);
        if (key.len == 0) {
            diag.line = line_no;
            return LoadError.EmptyDotenvKey;
        }

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

test "a malformed dotenv line fails naming its 1-based line number (Dimension 7.4)" {
    const alloc = std.testing.allocator;
    var merged: std.process.Environ.Map = .init(alloc);
    defer merged.deinit();
    var diag: DotenvDiagnostic = .{};
    // Line 3 lacks '='; lines 1-2 are a comment and a valid pair.
    const bad = "# header\nGOOD=1\nthis line has no equals\n";
    try std.testing.expectError(LoadError.InvalidDotenvLine, overlayDotEnvContent(&merged, bad, &diag));
    try std.testing.expectEqual(@as(usize, 3), diag.line);

    diag = .{};
    const empty_key = "A=1\n =2\n";
    try std.testing.expectError(LoadError.EmptyDotenvKey, overlayDotEnvContent(&merged, empty_key, &diag));
    try std.testing.expectEqual(@as(usize, 2), diag.line);
}

test "parseEnvBool: one trimmed grammar for every boolean env var (Dimension 4.3)" {
    try std.testing.expectEqual(EnvBool.yes, parseEnvBool(" true"));
    try std.testing.expectEqual(EnvBool.yes, parseEnvBool("TRUE"));
    try std.testing.expectEqual(EnvBool.yes, parseEnvBool("1"));
    try std.testing.expectEqual(EnvBool.no, parseEnvBool("false\n"));
    try std.testing.expectEqual(EnvBool.no, parseEnvBool("\t0"));
    try std.testing.expectEqual(EnvBool.invalid, parseEnvBool("yes"));
    try std.testing.expectEqual(EnvBool.invalid, parseEnvBool(""));
}

test "stripOptionalQuotes handles quoted and raw values" {
    try std.testing.expectEqualStrings("abc", stripOptionalQuotes("\"abc\""));
    try std.testing.expectEqualStrings("xyz", stripOptionalQuotes("'xyz'"));
    try std.testing.expectEqualStrings("plain", stripOptionalQuotes("plain"));
}
