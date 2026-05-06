//! Parse `BALANCE_EXHAUSTED_POLICY` env var. Drives the metering gate on a
//! tenant whose `billing.tenant_billing.balance_cents` has hit zero.

const std = @import("std");
const logging = @import("log");

const log = logging.scoped(.balance_policy);

const ENV_VAR_NAME = "BALANCE_EXHAUSTED_POLICY";

pub const Policy = enum {
    /// Log + let the run proceed. Zero cents deducted.
    @"continue",
    /// Same as `continue` plus a rate-limited activity event. Default.
    warn,
    /// Pre-claim gate rejects the delivery; zombie never runs.
    stop,

    pub fn label(self: Policy) []const u8 {
        return switch (self) {
            .@"continue" => "continue",
            .warn => "warn",
            .stop => "stop",
        };
    }
};

pub const DEFAULT: Policy = .warn;

pub fn parse(raw: []const u8) ?Policy {
    if (std.ascii.eqlIgnoreCase(raw, "continue")) return .@"continue";
    if (std.ascii.eqlIgnoreCase(raw, "warn")) return .warn;
    if (std.ascii.eqlIgnoreCase(raw, "stop")) return .stop;
    return null;
}

/// Pure resolution: null/unknown → DEFAULT (with warn log on unknown).
/// Split out from `resolveFromEnv` so tests can pin every branch without
/// round-tripping through libc setenv — short env values trigger a SIMD
/// over-read in `posix.getenv` that valgrind flags as invalid.
pub fn resolve(raw: ?[]const u8) Policy {
    const s = raw orelse return DEFAULT;
    return parse(s) orelse {
        log.warn("unknown_value", .{ .observed = s, .defaulting = DEFAULT.label() });
        return DEFAULT;
    };
}

/// Resolve from env. Absent / unknown values fall back to DEFAULT with a
/// startup warn log that names the observed value (so operators see why
/// they didn't get what they typed).
pub fn resolveFromEnv(alloc: std.mem.Allocator) Policy {
    const raw = std.process.getEnvVarOwned(alloc, ENV_VAR_NAME) catch |err| switch (err) {
        error.EnvironmentVariableNotFound => return resolve(null),
        else => {
            log.warn("env_read_failed", .{ .err = @errorName(err), .defaulting = DEFAULT.label() });
            return DEFAULT;
        },
    };
    defer alloc.free(raw);
    return resolve(raw);
}

// ── Tests ────────────────────────────────────────────────────────────────

test "parse: accepts known values, case-insensitive" {
    try std.testing.expectEqual(Policy.@"continue", parse("continue").?);
    try std.testing.expectEqual(Policy.warn, parse("warn").?);
    try std.testing.expectEqual(Policy.stop, parse("stop").?);
    try std.testing.expectEqual(Policy.@"continue", parse("CONTINUE").?);
    try std.testing.expectEqual(Policy.warn, parse("Warn").?);
}

test "parse: unknown returns null" {
    try std.testing.expect(parse("") == null);
    try std.testing.expect(parse("halt") == null);
    try std.testing.expect(parse("  warn  ") == null); // no trimming
}

test "DEFAULT is warn" {
    try std.testing.expectEqual(Policy.warn, DEFAULT);
}

test "label round-trips" {
    try std.testing.expectEqualStrings("continue", Policy.@"continue".label());
    try std.testing.expectEqualStrings("warn", Policy.warn.label());
    try std.testing.expectEqualStrings("stop", Policy.stop.label());
}

// ── resolve ───────────────────────────────────────────────────────────────
//
// resolve() is the pure env→Policy core consumed by worker.zig at startup.
// Tests exercise it directly; `resolveFromEnv` is a thin libc wrapper whose
// body is just `getEnvVarOwned + resolve`. Round-tripping through setenv in
// tests trips a Zig stdlib SIMD over-read inside posix.getenv that valgrind
// (memleak gate) flags as invalid for short env values.

test "resolve: null raw returns DEFAULT (env-absent branch)" {
    try std.testing.expectEqual(DEFAULT, resolve(null));
}

test "resolve: known tokens parse case-insensitively" {
    try std.testing.expectEqual(Policy.stop, resolve("stop"));
    try std.testing.expectEqual(Policy.warn, resolve("warn"));
    try std.testing.expectEqual(Policy.@"continue", resolve("continue"));
    try std.testing.expectEqual(Policy.stop, resolve("STOP"));
    try std.testing.expectEqual(Policy.@"continue", resolve("Continue"));
}

test "resolve: unknown / empty / whitespace falls back to DEFAULT" {
    // parse() is strict-eq (no trimming) — "  warn  " must NOT decode to warn.
    const garbage = [_][]const u8{ "halt", "", "  warn  ", "stop;DROP TABLE", "STOPPED" };
    for (garbage) |val| {
        try std.testing.expectEqual(DEFAULT, resolve(val));
    }
}
