//! M9_001 §5.0 — Outbound proxy unit tests.
//! Tests pure functions: extractDomain, extractPath, serviceForDomain, stripEcho, MAX_RESPONSE_BYTES.
//!
//! §5.2: Unknown domain → serviceForDomain returns null → DomainBlocked error path.
//! §5.3: MAX_RESPONSE_BYTES = 10 MB; stripEcho removes credential echoes.
//! §5.4: extractPath correctly handles scheme-prefixed targets (PR #205 fix #7).
//! §5.5: PipelineError.ApprovalRequired exists and is reachable (PR #205 fix #3).

const std = @import("std");
const pipeline = @import("outbound_proxy.zig");

// ── extractDomain ──────────────────────────────────────────────────────────

test "extractDomain: strips https scheme and path" {
    const d = pipeline.extractDomain("https://slack.com/api/chat.postMessage");
    try std.testing.expectEqualStrings("slack.com", d);
}

test "extractDomain: strips http scheme" {
    const d = pipeline.extractDomain("http://localhost:8080/path");
    try std.testing.expectEqualStrings("localhost:8080", d);
}

test "extractDomain: handles bare domain with path" {
    const d = pipeline.extractDomain("slack.com/api/chat");
    try std.testing.expectEqualStrings("slack.com", d);
}

test "extractDomain: handles bare domain with no path" {
    const d = pipeline.extractDomain("slack.com");
    try std.testing.expectEqualStrings("slack.com", d);
}

// ── serviceForDomain ────────────────────────────────────────────────────────

test "serviceForDomain: maps slack.com → slack" {
    try std.testing.expectEqualStrings("slack", pipeline.serviceForDomain("slack.com").?);
}

test "serviceForDomain: maps hooks.slack.com → slack" {
    try std.testing.expectEqualStrings("slack", pipeline.serviceForDomain("hooks.slack.com").?);
}

test "serviceForDomain: maps gmail.googleapis.com → gmail" {
    try std.testing.expectEqualStrings("gmail", pipeline.serviceForDomain("gmail.googleapis.com").?);
}

test "serviceForDomain: maps api.agentmail.to → agentmail" {
    try std.testing.expectEqualStrings("agentmail", pipeline.serviceForDomain("api.agentmail.to").?);
}

test "serviceForDomain: maps discord.com → discord" {
    try std.testing.expectEqualStrings("discord", pipeline.serviceForDomain("discord.com").?);
}

test "serviceForDomain: maps grafana.com → grafana" {
    try std.testing.expectEqualStrings("grafana", pipeline.serviceForDomain("grafana.com").?);
}

// §5.2: unreachable domain → null → caller maps to DomainBlocked → HTTP 502
test "serviceForDomain: returns null for unknown domain (§5.2)" {
    try std.testing.expect(pipeline.serviceForDomain("attacker.example.com") == null);
}

test "serviceForDomain: returns null for mycompany.grafana.com (subdomain not in allowlist)" {
    try std.testing.expect(pipeline.serviceForDomain("mycompany.grafana.com") == null);
}

test "serviceForDomain: returns null for empty string" {
    try std.testing.expect(pipeline.serviceForDomain("") == null);
}

// ── MAX_RESPONSE_BYTES (§5.3) ──────────────────────────────────────────────

test "MAX_RESPONSE_BYTES is exactly 10 MB" {
    try std.testing.expectEqual(@as(usize, 10 * 1024 * 1024), pipeline.MAX_RESPONSE_BYTES);
}

// ── stripEcho (§5.3) ────────────────────────────────────────────────────────

test "stripEcho: removes credential from response body" {
    const alloc = std.testing.allocator;
    const body = "ok response xoxb-my-slack-token end";
    const cleaned = try pipeline.stripEcho(alloc, body, "xoxb-my-slack-token");
    defer alloc.free(cleaned);
    try std.testing.expect(std.mem.indexOf(u8, cleaned, "xoxb-my-slack-token") == null);
    try std.testing.expect(std.mem.indexOf(u8, cleaned, "[REDACTED]") != null);
}

test "stripEcho: returns body unchanged when credential not present" {
    const alloc = std.testing.allocator;
    const body = "clean response body";
    const cleaned = try pipeline.stripEcho(alloc, body, "xoxb-secret");
    defer alloc.free(cleaned);
    try std.testing.expectEqualStrings(body, cleaned);
}

test "stripEcho: removes all occurrences of credential" {
    const alloc = std.testing.allocator;
    const body = "sec sec sec";
    const cleaned = try pipeline.stripEcho(alloc, body, "sec");
    defer alloc.free(cleaned);
    try std.testing.expect(std.mem.indexOf(u8, cleaned, "sec") == null);
    try std.testing.expectEqual(@as(usize, 3), std.mem.count(u8, cleaned, "[REDACTED]"));
}

test "stripEcho: empty credential returns body unchanged (guard)" {
    // Pipeline never injects an empty credential; guard returns body as-is.
    const alloc = std.testing.allocator;
    const body = "response body";
    const cleaned = try pipeline.stripEcho(alloc, body, "");
    defer alloc.free(cleaned);
    try std.testing.expectEqualStrings(body, cleaned);
}

// ── extractPath (§5.4 — PR #205 fix #7) ─────────────────────────────────────
// Before the fix, path_start did not account for scheme_len, producing a wrong
// firewall path ("/slack.com/api/chat" from "https://slack.com/api/chat").
// After the fix, extractPath(target, domain) correctly strips scheme + domain.

test "extractPath: https target gives correct path starting with slash" {
    const p = pipeline.extractPath("https://slack.com/api/chat.postMessage", "slack.com");
    try std.testing.expectEqualStrings("/api/chat.postMessage", p);
}

test "extractPath: http target gives correct path starting with slash" {
    const p = pipeline.extractPath("http://localhost:8080/v1/resource", "localhost:8080");
    try std.testing.expectEqualStrings("/v1/resource", p);
}

test "extractPath: bare target without scheme gives correct path" {
    const p = pipeline.extractPath("slack.com/api/chat", "slack.com");
    try std.testing.expectEqualStrings("/api/chat", p);
}

test "extractPath: target with no path after domain returns /" {
    const p = pipeline.extractPath("slack.com", "slack.com");
    try std.testing.expectEqualStrings("/", p);
}

test "extractPath: https target with no path after domain returns /" {
    const p = pipeline.extractPath("https://slack.com", "slack.com");
    try std.testing.expectEqualStrings("/", p);
}

test "extractPath: deep nested path preserved exactly" {
    const p = pipeline.extractPath("https://discord.com/api/webhooks/123/abc", "discord.com");
    try std.testing.expectEqualStrings("/api/webhooks/123/abc", p);
}

// ── PipelineError.ApprovalRequired (§5.5 — PR #205 fix #3) ──────────────────
// Verify that ApprovalRequired is a valid member of PipelineError so the
// .requires_approval firewall case is never silently dropped.

test "PipelineError.ApprovalRequired: error exists in PipelineError union" {
    // Comptime-reachable: if someone removes ApprovalRequired this fails to compile.
    const err: pipeline.PipelineError = pipeline.PipelineError.ApprovalRequired;
    try std.testing.expect(err == pipeline.PipelineError.ApprovalRequired);
}

test "PipelineError: exhaustive switch compiles — all 9 members present" {
    // Regression guard: adding/removing an error variant breaks the exhaustive switch.
    const dummy: pipeline.PipelineError = error.DomainBlocked;
    const n = switch (dummy) {
        error.DomainBlocked      => 1,
        error.InjectionDetected  => 2,
        error.ApprovalRequired   => 3,
        error.GrantNotFound      => 4,
        error.GrantPending       => 5,
        error.GrantDenied        => 6,
        error.CredentialNotFound => 7,
        error.TargetError        => 8,
        error.OutOfMemory        => 9,
    };
    try std.testing.expectEqual(@as(usize, 1), n);
}
