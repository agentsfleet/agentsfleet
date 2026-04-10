//! Domain allowlist enforcement for the AI Firewall.
//!
//! Before any outbound HTTP call, the firewall checks the target domain
//! against the Zombie's declared skill domains. Requests to undeclared
//! domains are blocked. This is defense-in-depth on top of the network
//! policy (bwrap/nftables).

const std = @import("std");

pub const FirewallDecision = union(enum) {
    allow: void,
    block: struct { reason: []const u8 },
    requires_approval: struct { reason: []const u8 },
};

/// Check whether `target` is in `allowed_domains`.
/// Comparison is case-insensitive and exact-match (no suffix matching).
pub fn checkDomain(allowed_domains: []const []const u8, target: []const u8) FirewallDecision {
    for (allowed_domains) |allowed| {
        if (asciiEqlIgnoreCase(allowed, target)) return .{ .allow = {} };
    }
    return .{ .block = .{ .reason = "Domain not in allowlist" } };
}

fn asciiEqlIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |ca, cb| {
        if (std.ascii.toLower(ca) != std.ascii.toLower(cb)) return false;
    }
    return true;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "1.1: allowed domain returns Allow" {
    const allowed = &[_][]const u8{ "api.slack.com", "api.github.com" };
    const result = checkDomain(allowed, "api.slack.com");
    try std.testing.expect(result == .allow);
}

test "1.2: unknown domain returns Block" {
    const allowed = &[_][]const u8{"api.slack.com"};
    const result = checkDomain(allowed, "evil.com");
    switch (result) {
        .block => |b| try std.testing.expect(std.mem.indexOf(u8, b.reason, "not in allowlist") != null),
        else => return error.ExpectedBlock,
    }
}

test "1.3: subdomain spoofing blocked (exact match, not suffix)" {
    const allowed = &[_][]const u8{"api.slack.com"};
    const result = checkDomain(allowed, "api.slack.com.evil.com");
    try std.testing.expect(result == .block);
}

test "1.4: case-insensitive comparison allows uppercase" {
    const allowed = &[_][]const u8{"api.slack.com"};
    const result = checkDomain(allowed, "API.SLACK.COM");
    try std.testing.expect(result == .allow);
}

test "empty allowlist blocks everything" {
    const allowed = &[_][]const u8{};
    const result = checkDomain(allowed, "api.slack.com");
    try std.testing.expect(result == .block);
}
