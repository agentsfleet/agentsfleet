//! Parse `BALANCE_EXHAUSTED_POLICY` env var. Drives the metering gate on a
//! tenant whose `billing.tenant_billing.balance_cents` has hit zero.

const std = @import("std");

const log = std.log.scoped(.balance_policy);

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

/// Resolve from env. Absent / unknown values fall back to DEFAULT with a
/// startup warn log that names the observed value (so operators see why
/// they didn't get what they typed).
pub fn resolveFromEnv(alloc: std.mem.Allocator) Policy {
    const raw = std.process.getEnvVarOwned(alloc, ENV_VAR_NAME) catch |err| switch (err) {
        error.EnvironmentVariableNotFound => return DEFAULT,
        else => {
            log.warn("balance_policy.env_read_err err={s} defaulting={s}", .{ @errorName(err), DEFAULT.label() });
            return DEFAULT;
        },
    };
    defer alloc.free(raw);
    return parse(raw) orelse {
        log.warn("balance_policy.unknown_value observed=\"{s}\" defaulting={s}", .{ raw, DEFAULT.label() });
        return DEFAULT;
    };
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

// ── resolveFromEnv ────────────────────────────────────────────────────────
//
// resolveFromEnv() is the env→Policy wire consumed by worker_zombie.zig at
// every zombie spawn. The env var name and the default-on-missing/unknown
// contract is operator-facing — tests pin all three branches.

test "resolveFromEnv: returns DEFAULT when BALANCE_EXHAUSTED_POLICY is unset" {
    // Branch: env var absent → catch error.EnvironmentVariableNotFound → DEFAULT.
    // Bug catch: regression that removes the catch arm and causes startup to
    // panic when operators haven't set the env var.
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.unsetenv("BALANCE_EXHAUSTED_POLICY");
    try std.testing.expectEqual(DEFAULT, resolveFromEnv(std.testing.allocator));
}

test "resolveFromEnv: returns parsed value for each known token (case-insensitive)" {
    // Branch: env var read → parse() succeeds → that policy.
    // Bug catch: regression that breaks the env→enum wire, e.g. someone renames
    // the env var or changes parse() to be case-sensitive while the env-side
    // contract keeps case-insensitive.
    const c = @cImport(@cInclude("stdlib.h"));
    defer _ = c.unsetenv("BALANCE_EXHAUSTED_POLICY");

    const cases = [_]struct { raw: [*:0]const u8, expected: Policy }{
        .{ .raw = "stop", .expected = .stop },
        .{ .raw = "warn", .expected = .warn },
        .{ .raw = "continue", .expected = .@"continue" },
        .{ .raw = "STOP", .expected = .stop }, // case-insensitive contract
        .{ .raw = "Continue", .expected = .@"continue" },
    };
    for (cases) |case| {
        _ = c.setenv("BALANCE_EXHAUSTED_POLICY", case.raw, 1);
        try std.testing.expectEqual(case.expected, resolveFromEnv(std.testing.allocator));
    }
}

test "resolveFromEnv: falls back to DEFAULT on unknown / empty / whitespace values" {
    // Branch: env var read → parse() returns null → log + DEFAULT.
    // Bug catch: regression that returns the wrong value or panics on garbage
    // input. parse() rejects whitespace-padded tokens (no trimming), so
    // "  warn  " must NOT be accepted as warn — the contract is strict-eq.
    const c = @cImport(@cInclude("stdlib.h"));
    defer _ = c.unsetenv("BALANCE_EXHAUSTED_POLICY");

    const garbage = [_][*:0]const u8{ "halt", "", "  warn  ", "stop;DROP TABLE", "STOPPED" };
    for (garbage) |val| {
        _ = c.setenv("BALANCE_EXHAUSTED_POLICY", val, 1);
        try std.testing.expectEqual(DEFAULT, resolveFromEnv(std.testing.allocator));
    }
}
