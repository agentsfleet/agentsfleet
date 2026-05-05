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
