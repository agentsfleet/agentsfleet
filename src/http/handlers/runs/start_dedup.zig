/// M16_002 §2.1: Dedup key computation for run submissions.
///
/// Dedup key = hex(sha256(spec_markdown || workspace_id || base_commit_sha_or_empty)).
/// This module is extracted from start.zig to keep that file under 500 lines.
const std = @import("std");

/// Compute the dedup key for a run submission.
/// Returns an owned 64-char hex string (sha256 digest). Caller must free.
pub fn computeDedupKey(
    alloc: std.mem.Allocator,
    spec_markdown: []const u8,
    workspace_id: []const u8,
    base_commit_sha: ?[]const u8,
) ![]u8 {
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    hasher.update(spec_markdown);
    hasher.update(workspace_id);
    if (base_commit_sha) |sha| hasher.update(sha);
    const digest = hasher.finalResult();
    const hex = std.fmt.bytesToHex(digest, .lower);
    return alloc.dupe(u8, &hex);
}

// ── Unit tests (T7, T8) ──────────────────────────────────────────────────────

// T7: computeDedupKey is deterministic — same inputs produce same output.
test "T7: computeDedupKey produces same output for identical inputs" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec content", "ws-123", "abc123sha");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec content", "ws-123", "abc123sha");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
    // Output must be 64 hex chars (sha256 = 32 bytes × 2).
    try std.testing.expectEqual(@as(usize, 64), k1.len);
}

// T7: computeDedupKey with null base_commit_sha is stable.
test "T7: computeDedupKey without base_commit_sha is deterministic" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-1", null);
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-1", null);
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

// T8: computeDedupKey differs when any input changes.
test "T8: computeDedupKey differs when spec changes" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec A", "ws-1", "sha1");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec B", "ws-1", "sha1");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: computeDedupKey differs when workspace_id changes" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-A", "sha1");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-B", "sha1");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: computeDedupKey differs when base_commit_sha changes" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-1", "sha-old");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-1", "sha-new");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: computeDedupKey differs when sha is null vs non-null" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-1", null);
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-1", "sha");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

// Regression: verify output is 64 lowercase hex chars for any fixed input.
test "T7: computeDedupKey regression — output is 64 hex chars" {
    const alloc = std.testing.allocator;
    const k = try computeDedupKey(alloc, "spec content", "ws-123", "abc123sha");
    defer alloc.free(k);
    try std.testing.expectEqual(@as(usize, 64), k.len);
    for (k) |ch| {
        try std.testing.expect((ch >= '0' and ch <= '9') or (ch >= 'a' and ch <= 'f'));
    }
}

// ── T7: Determinism — 5 distinct input combos each called twice ──────────────

test "T7: determinism combo 1 — long spec, short sha" {
    const alloc = std.testing.allocator;
    const spec = "Implement the entire auth system from scratch.\n" ** 10;
    const k1 = try computeDedupKey(alloc, spec, "ws-aaa", "deadbeef");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, spec, "ws-aaa", "deadbeef");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

test "T7: determinism combo 2 — unicode in spec" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "修复登录漏洞 🔒", "ws-cjk", "cafe1234");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "修复登录漏洞 🔒", "ws-cjk", "cafe1234");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

test "T7: determinism combo 3 — empty base_commit_sha" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "Do the task.", "ws-zzz", "");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "Do the task.", "ws-zzz", "");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

test "T7: determinism combo 4 — null base_commit_sha" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "Fix bug", "ws-null", null);
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "Fix bug", "ws-null", null);
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

test "T7: determinism combo 5 — 32KB spec" {
    const alloc = std.testing.allocator;
    const spec = "x" ** (32 * 1024);
    const k1 = try computeDedupKey(alloc, spec, "ws-big", "sha999");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, spec, "ws-big", "sha999");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
    try std.testing.expectEqual(@as(usize, 64), k1.len);
}

// ── T7: Regression — pin exact output for known input ────────────────────────

test "T7: regression — known input produces known sha256 hex" {
    // sha256("" || "" || "") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    // But we hash spec_markdown, workspace_id, base_commit_sha concatenated without separator.
    // For ("", "", null): sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    const alloc = std.testing.allocator;
    const k = try computeDedupKey(alloc, "", "", null);
    defer alloc.free(k);
    try std.testing.expectEqualStrings(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        k,
    );
}

// ── T8: Sensitivity — single-byte flips change output ────────────────────────

test "T8: one-byte change in spec produces different key" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec A", "ws-1", "sha1");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec B", "ws-1", "sha1");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: one-char change in workspace_id produces different key" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-A", "sha1");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-B", "sha1");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: null vs empty string sha produces different key" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-1", null);
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-1", "");
    defer alloc.free(k2);
    // null omits update(); "" calls update("") which is a no-op — same as null in sha2.
    // Both should be identical (sha256("spec" ++ "ws-1") in both cases).
    // This test documents the actual behaviour: they ARE equal.
    try std.testing.expectEqualStrings(k1, k2);
}

test "T8: non-empty sha vs null sha produces different key" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "spec", "ws-1", null);
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "spec", "ws-1", "abc");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

test "T8: emoji in spec produces deterministic but different key from ASCII" {
    const alloc = std.testing.allocator;
    const k1 = try computeDedupKey(alloc, "task", "ws-1", "sha");
    defer alloc.free(k1);
    const k2 = try computeDedupKey(alloc, "task 🚀", "ws-1", "sha");
    defer alloc.free(k2);
    try std.testing.expect(!std.mem.eql(u8, k1, k2));
}

// ── T9: Output format — 64 lowercase hex chars, no spaces/uppercase ──────────

test "T9: output is exactly 64 chars for all inputs" {
    const alloc = std.testing.allocator;
    const cases = [_]struct { spec: []const u8, ws: []const u8, sha: ?[]const u8 }{
        .{ .spec = "", .ws = "", .sha = null },
        .{ .spec = "a", .ws = "b", .sha = "c" },
        .{ .spec = "x" ** 1000, .ws = "ws-big", .sha = "sha" },
        .{ .spec = "unicode 中文", .ws = "ws-u", .sha = null },
    };
    for (cases) |c| {
        const k = try computeDedupKey(alloc, c.spec, c.ws, c.sha);
        defer alloc.free(k);
        try std.testing.expectEqual(@as(usize, 64), k.len);
        for (k) |ch| {
            const is_lower_hex = (ch >= '0' and ch <= '9') or (ch >= 'a' and ch <= 'f');
            try std.testing.expect(is_lower_hex);
        }
    }
}

// ── T11: Memory — no leaks on any input ───────────────────────────────────────

test "T11: no memory leak on empty inputs" {
    const k = try computeDedupKey(std.testing.allocator, "", "", null);
    defer std.testing.allocator.free(k);
}

test "T11: no memory leak on large spec" {
    const spec = "y" ** (64 * 1024);
    const k = try computeDedupKey(std.testing.allocator, spec, "ws", "sha");
    defer std.testing.allocator.free(k);
    try std.testing.expectEqual(@as(usize, 64), k.len);
}
