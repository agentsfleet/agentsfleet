//! API endpoint policy for the AI Firewall.
//!
//! Per-endpoint rules from the TRIGGER.md `firewall:` section.
//! Each rule specifies domain, HTTP method, path pattern (glob), and action.
//! Endpoint rules are more specific than domain-level checks and take precedence.
//! Default for unlisted endpoints on allowed domains: allow.

const std = @import("std");
const domain_policy = @import("domain_policy.zig");
const FirewallDecision = domain_policy.FirewallDecision;
const asciiEqlIgnoreCase = domain_policy.asciiEqlIgnoreCase;
const Allocator = std.mem.Allocator;

const log = std.log.scoped(.firewall);

const EndpointAction = enum {
    allow,
    deny,
    approve,
};

pub const EndpointRule = struct {
    domain: []const u8,
    method: []const u8,
    path: []const u8,
    action: EndpointAction,
    reason: []const u8,
};

/// Check a request against endpoint rules.
/// If a rule matches, its action determines the decision.
/// If no rule matches, returns Allow (caller handles domain-level check separately).
pub fn checkEndpoint(
    rules: []const EndpointRule,
    method: []const u8,
    domain: []const u8,
    path: []const u8,
) FirewallDecision {
    for (rules) |rule| {
        if (!asciiEqlIgnoreCase(rule.domain, domain)) continue;
        if (!asciiEqlIgnoreCase(rule.method, method)) continue;
        if (!globMatch(rule.path, path)) continue;

        return switch (rule.action) {
            .allow => .{ .allow = {} },
            .deny => .{ .block = .{ .reason = rule.reason } },
            .approve => .{ .requires_approval = .{ .reason = rule.reason } },
        };
    }
    return .{ .allow = {} };
}

/// Parse endpoint rules from a JSON firewall config.
/// Expected shape: `{"endpoint_rules": [{"domain":..., "method":..., "path":..., "action":..., "reason":...}]}`.
/// Caller owns returned slice and must free each rule's strings + the slice itself.
pub fn parseEndpointRules(alloc: Allocator, json_bytes: []const u8) ![]EndpointRule {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json_bytes, .{}) catch
        return error.FirewallPolicyParseError;
    defer parsed.deinit();

    const root = parsed.value;
    if (root != .object) return error.FirewallPolicyParseError;

    const rules_val = root.object.get("endpoint_rules") orelse return error.FirewallPolicyParseError;
    if (rules_val != .array) return error.FirewallPolicyParseError;

    var result: std.ArrayList(EndpointRule) = .{};
    errdefer {
        for (result.items) |r| freeRule(alloc, r);
        result.deinit(alloc);
    }

    for (rules_val.array.items) |item| {
        if (item != .object) continue;

        const domain = try dupeJsonStr(alloc, item, "domain");
        errdefer alloc.free(domain);
        const method = try dupeJsonStr(alloc, item, "method");
        errdefer alloc.free(method);
        const path = try dupeJsonStr(alloc, item, "path");
        errdefer alloc.free(path);
        if (hasMidPathWildcard(path)) {
            log.warn("firewall.endpoint_rule_unsupported_wildcard path={s} hint=only leading/trailing * supported", .{path});
            return error.FirewallPolicyParseError;
        }
        const action_str = jsonStr(item, "action") orelse return error.FirewallPolicyParseError;
        const reason = try dupeJsonStr(alloc, item, "reason");
        errdefer alloc.free(reason);

        try result.append(alloc, .{
            .domain = domain,
            .method = method,
            .path = path,
            .action = parseAction(action_str),
            .reason = reason,
        });
    }
    return try result.toOwnedSlice(alloc);
}

pub fn freeRules(alloc: Allocator, rules: []EndpointRule) void {
    for (rules) |r| freeRule(alloc, r);
    alloc.free(rules);
}

fn freeRule(alloc: Allocator, r: EndpointRule) void {
    alloc.free(r.domain);
    alloc.free(r.method);
    alloc.free(r.path);
    alloc.free(r.reason);
}

/// Reject paths with mid-path wildcards (e.g., `/api/*/users`).
/// Only leading `*X`, trailing `X*`, both `*X*`, and lone `*` are supported.
fn hasMidPathWildcard(path: []const u8) bool {
    if (path.len <= 1) return false;
    // Lone `*` is fine
    if (std.mem.eql(u8, path, "*")) return false;
    // Check for `*` in positions that are not the first or last character
    for (path[1 .. path.len - 1]) |c| {
        if (c == '*') return true;
    }
    return false;
}

fn parseAction(s: []const u8) EndpointAction {
    if (asciiEqlIgnoreCase(s, "deny")) return .deny;
    if (asciiEqlIgnoreCase(s, "approve")) return .approve;
    return .allow;
}

fn jsonStr(val: std.json.Value, key: []const u8) ?[]const u8 {
    if (val != .object) return null;
    const v = val.object.get(key) orelse return null;
    return if (v == .string) v.string else null;
}

fn dupeJsonStr(alloc: Allocator, val: std.json.Value, key: []const u8) ![]const u8 {
    const s = jsonStr(val, key) orelse return error.FirewallPolicyParseError;
    return try alloc.dupe(u8, s);
}

/// Simple glob matching: `*` matches any substring.
/// Only supports leading/trailing/both `*` — not mid-path wildcards.
fn globMatch(pattern: []const u8, text: []const u8) bool {
    if (std.mem.eql(u8, pattern, "*")) return true;

    if (pattern.len >= 2 and pattern[0] == '*' and pattern[pattern.len - 1] == '*') {
        const inner = pattern[1 .. pattern.len - 1];
        return std.mem.indexOf(u8, text, inner) != null;
    }

    if (pattern.len >= 1 and pattern[pattern.len - 1] == '*') {
        const prefix = pattern[0 .. pattern.len - 1];
        return text.len >= prefix.len and std.mem.eql(u8, text[0..prefix.len], prefix);
    }

    if (pattern.len >= 1 and pattern[0] == '*') {
        const suffix = pattern[1..];
        return text.len >= suffix.len and std.mem.eql(u8, text[text.len - suffix.len ..], suffix);
    }

    return std.mem.eql(u8, pattern, text);
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "2.1: deny rule blocks matching request" {
    const rules = &[_]EndpointRule{.{
        .domain = "api.stripe.com",
        .method = "POST",
        .path = "/v1/refunds*",
        .action = .deny,
        .reason = "Refunds require manual processing",
    }};
    const result = checkEndpoint(rules, "POST", "api.stripe.com", "/v1/refunds");
    switch (result) {
        .block => |b| try std.testing.expectEqualStrings("Refunds require manual processing", b.reason),
        else => return error.ExpectedBlock,
    }
}

test "2.2: no matching rule returns Allow (default for allowed domains)" {
    const rules = &[_]EndpointRule{.{
        .domain = "api.stripe.com",
        .method = "POST",
        .path = "/v1/refunds*",
        .action = .deny,
        .reason = "Refunds require manual processing",
    }};
    const result = checkEndpoint(rules, "GET", "api.stripe.com", "/v1/charges");
    try std.testing.expect(result == .allow);
}

test "2.3: approve rule returns RequiresApproval" {
    const rules = &[_]EndpointRule{.{
        .domain = "api.github.com",
        .method = "DELETE",
        .path = "*",
        .action = .approve,
        .reason = "Delete operations need human approval",
    }};
    const result = checkEndpoint(rules, "DELETE", "api.github.com", "/repos/org/repo");
    switch (result) {
        .requires_approval => |a| try std.testing.expectEqualStrings("Delete operations need human approval", a.reason),
        else => return error.ExpectedApproval,
    }
}

test "2.4: parseEndpointRules parses valid JSON" {
    const alloc = std.testing.allocator;
    const json =
        \\{"endpoint_rules": [
        \\  {"domain": "api.stripe.com", "method": "POST", "path": "/v1/refunds*", "action": "deny", "reason": "No refunds"},
        \\  {"domain": "api.github.com", "method": "DELETE", "path": "*", "action": "approve", "reason": "Needs approval"},
        \\  {"domain": "api.slack.com", "method": "GET", "path": "/api/*", "action": "allow", "reason": "Read OK"}
        \\]}
    ;
    const rules = try parseEndpointRules(alloc, json);
    defer freeRules(alloc, rules);

    try std.testing.expectEqual(@as(usize, 3), rules.len);
    try std.testing.expectEqualStrings("api.stripe.com", rules[0].domain);
    try std.testing.expect(rules[0].action == .deny);
    try std.testing.expect(rules[1].action == .approve);
    try std.testing.expect(rules[2].action == .allow);
}

test "globMatch: exact" {
    try std.testing.expect(globMatch("/v1/charges", "/v1/charges"));
    try std.testing.expect(!globMatch("/v1/charges", "/v1/refunds"));
}

test "globMatch: trailing wildcard" {
    try std.testing.expect(globMatch("/v1/refunds*", "/v1/refunds"));
    try std.testing.expect(globMatch("/v1/refunds*", "/v1/refunds/abc"));
    try std.testing.expect(!globMatch("/v1/refunds*", "/v1/charges"));
}

test "globMatch: star-only matches everything" {
    try std.testing.expect(globMatch("*", "/any/path"));
    try std.testing.expect(globMatch("*", ""));
}
