//! Parse `BALANCE_EXHAUSTED_POLICY` env var. Drives the metering gate on a
//! tenant whose `billing.tenant_billing.balance_nanos` has hit zero.

const std = @import("std");
const common = @import("common");
const logging = @import("log");

const log = logging.scoped(.balance_policy);

const EnvMap = common.env.Map;

const ENV_VAR_NAME = "BALANCE_EXHAUSTED_POLICY";

const S_CONTINUE = "continue";
const S_STOP = "stop";
const S_WARN = "warn";

pub const Policy = enum {
    /// Log + let the run proceed. Zero nanos deducted.
    @"continue",
    /// Same as `continue` plus a rate-limited activity event. Default.
    warn,
    /// Pre-claim gate rejects the delivery; zombie never runs.
    stop,

    pub fn label(self: Policy) []const u8 {
        return switch (self) {
            .@"continue" => S_CONTINUE,
            .warn => S_WARN,
            .stop => S_STOP,
        };
    }
};

pub const DEFAULT: Policy = .warn;

pub fn parse(raw: []const u8) ?Policy {
    if (std.ascii.eqlIgnoreCase(raw, S_CONTINUE)) return .@"continue";
    if (std.ascii.eqlIgnoreCase(raw, S_WARN)) return .warn;
    if (std.ascii.eqlIgnoreCase(raw, S_STOP)) return .stop;
    return null;
}

/// Pure resolution: null/unknown → DEFAULT (with warn log on unknown).
/// Split out from `resolveFromEnv` so tests can pin every branch directly
/// without constructing an env map.
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
pub fn resolveFromEnv(env_map: *const EnvMap, alloc: std.mem.Allocator) Policy {
    const raw = (common.env.owned(env_map, alloc, ENV_VAR_NAME) catch |err| {
        log.warn("env_read_failed", .{ .err = @errorName(err), .defaulting = DEFAULT.label() });
        return DEFAULT;
    }) orelse return resolve(null);
    defer alloc.free(raw);
    return resolve(raw);
}

// ── Tests ────────────────────────────────────────────────────────────────

test "wire labels are the contract" {
    // pin test: literal is the contract
    try std.testing.expectEqualStrings("continue", Policy.@"continue".label());
    // pin test: literal is the contract
    try std.testing.expectEqualStrings("warn", Policy.warn.label());
    // pin test: literal is the contract
    try std.testing.expectEqualStrings("stop", Policy.stop.label());
}

test "parse: every variant round-trips through its label" {
    inline for (.{ Policy.@"continue", Policy.warn, Policy.stop }) |p| {
        try std.testing.expectEqual(p, parse(p.label()).?);
    }
}

test "parse: case-insensitive" {
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

// ── resolve ───────────────────────────────────────────────────────────────
//
// resolve() is the pure env→Policy core read at serve startup. Tests exercise
// it directly; `resolveFromEnv` is a thin wrapper whose body is just
// `common.env.owned + resolve`, so the value-mapping branches need no env map.

test "resolve: null raw returns DEFAULT (env-absent branch)" {
    try std.testing.expectEqual(DEFAULT, resolve(null));
}

test "resolve: every variant decodes from its label" {
    inline for (.{ Policy.@"continue", Policy.warn, Policy.stop }) |p| {
        try std.testing.expectEqual(p, resolve(p.label()));
    }
}

test "resolve: case-insensitive" {
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
