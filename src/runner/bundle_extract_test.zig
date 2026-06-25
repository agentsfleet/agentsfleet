//! Unit tests for runner-side Fleet Bundle extraction + cache (`bundle_extract.zig`).
//! Crafted in-memory tars (mirror `github_source_test.buildTar`) are untarred into a
//! real per-test temp workspace, plus a cache write/read round-trip. The HTTP download
//! path (`materialize`/`downloadBundle`) is integration-covered (it needs a live cp).

const std = @import("std");
const bundle_extract = @import("bundle_extract.zig");

// pin test: these literals ARE the canonical-tar entry names the runner must skip.
const SKILL_NAME = "SKILL.md";
const TRIGGER_NAME = "TRIGGER.md";

// Real security-reviewer bundle fixture (tests/fixtures/fleetbundle/security-reviewer/),
// wired as named @embedFile imports by src/build/fixtures.zig. It carries a nested
// support file (checklists/owasp.md), so the extraction test exercises folder
// materialization against a real bundle's bytes rather than synthetic entries.
const SR_SKILL = @embedFile("security-reviewer-SKILL.md");
const SR_TRIGGER = @embedFile("security-reviewer-TRIGGER.md");
const SR_OWASP = @embedFile("security-reviewer-owasp.md");
const SR_NESTED = "checklists/owasp.md";

const TarEntry = struct {
    name: []const u8,
    content: []const u8 = "",
    symlink_to: ?[]const u8 = null,
};

fn buildTar(alloc: std.mem.Allocator, entries: []const TarEntry) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    var tw: std.tar.Writer = .{ .underlying_writer = &aw.writer };
    for (entries) |e| {
        if (e.symlink_to) |target| {
            try tw.writeLink(e.name, target, .{});
        } else {
            try tw.writeFileBytes(e.name, e.content, .{});
        }
    }
    try aw.writer.flush();
    return aw.toOwnedSlice();
}

/// Fresh absolute temp dir for one test; deletes any stale tree first and on exit.
fn freshDir(io: std.Io, comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-be-test-" ++ name;
    try std.Io.Dir.cwd().deleteTree(io, path); // idempotent on a missing path
    try std.Io.Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

test "extractSupportFiles writes support files and folders, skips SKILL/TRIGGER" {
    const alloc = std.testing.allocator;
    const io = @import("common").globalIo();
    const ws = try freshDir(io, "extract");
    // Real security-reviewer bundle shape: SKILL.md + TRIGGER.md (both skipped),
    // a top-level README support file, and the nested checklists/owasp.md.
    const tar = try buildTar(alloc, &.{
        .{ .name = SKILL_NAME, .content = SR_SKILL },
        .{ .name = TRIGGER_NAME, .content = SR_TRIGGER },
        .{ .name = "README.md", .content = "readme" },
        .{ .name = SR_NESTED, .content = SR_OWASP },
    });
    defer alloc.free(tar);

    const written = try bundle_extract.extractSupportFiles(io, alloc, tar, ws);
    try std.testing.expectEqual(@as(usize, 2), written);

    // Support file + nested folder materialized with content...
    const readme = try std.Io.Dir.cwd().readFileAlloc(io, "/tmp/agentsfleet-be-test-extract/README.md", alloc, .limited(1024));
    defer alloc.free(readme);
    try std.testing.expectEqualStrings("readme", readme);
    const nested = try std.Io.Dir.cwd().readFileAlloc(io, "/tmp/agentsfleet-be-test-extract/" ++ SR_NESTED, alloc, .limited(64 * 1024));
    defer alloc.free(nested);
    try std.testing.expectEqualStrings(SR_OWASP, nested);

    // ...but SKILL.md/TRIGGER.md are NOT written (the lease carries the authoritative copy).
    try std.testing.expectError(error.FileNotFound, std.Io.Dir.cwd().access(io, "/tmp/agentsfleet-be-test-extract/SKILL.md", .{}));
    try std.testing.expectError(error.FileNotFound, std.Io.Dir.cwd().access(io, "/tmp/agentsfleet-be-test-extract/TRIGGER.md", .{}));
}

test "extractSupportFiles rejects path traversal" {
    const alloc = std.testing.allocator;
    const io = @import("common").globalIo();
    const ws = try freshDir(io, "traversal");
    const tar = try buildTar(alloc, &.{.{ .name = "../evil.md", .content = "x" }});
    defer alloc.free(tar);
    try std.testing.expectError(error.UnsafePath, bundle_extract.extractSupportFiles(io, alloc, tar, ws));
}

test "extractSupportFiles rejects symlink entries" {
    const alloc = std.testing.allocator;
    const io = @import("common").globalIo();
    const ws = try freshDir(io, "symlink");
    const tar = try buildTar(alloc, &.{.{ .name = "link", .symlink_to = "/etc/passwd" }});
    defer alloc.free(tar);
    try std.testing.expectError(error.UnsafePath, bundle_extract.extractSupportFiles(io, alloc, tar, ws));
}

test "writeCache then readCache round-trips the tar bytes" {
    const alloc = std.testing.allocator;
    const io = @import("common").globalIo();
    const base = try freshDir(io, "cache"); // A per-lease workspace under the base (where the temp file lands before rename).
    const ws = "/tmp/agentsfleet-be-test-cache/lease-1";
    try std.Io.Dir.createDirAbsolute(io, ws, .default_dir);

    const hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const tar = try buildTar(alloc, &.{.{ .name = "README.md", .content = "cached" }});
    defer alloc.free(tar);

    try bundle_extract.writeCache(io, base, ws, hash, tar);
    const got = bundle_extract.readCache(io, alloc, base, hash) orelse return error.TestUnexpectedResult;
    defer alloc.free(got);
    try std.testing.expectEqualSlices(u8, tar, got);
}

test "readCache misses (returns null) when no cache file exists" {
    const alloc = std.testing.allocator;
    const io = @import("common").globalIo();
    const base = try freshDir(io, "cache-miss");
    const hash = "0000000000000000000000000000000000000000000000000000000000000000";
    try std.testing.expect(bundle_extract.readCache(io, alloc, base, hash) == null);
}

const MAX_BUNDLE_TAR_BYTES: usize = 4 * 1024 * 1024; // mirror of bundle_extract's cap

test "accumulateBytes rejects a corrupt/oversized/overflowing size field, no panic (M100)" {
    // A value that doesn't fit usize (corrupt header) → TooLarge, never @intCast panic.
    if (@sizeOf(usize) < @sizeOf(u64)) {
        try std.testing.expectError(error.TooLarge, bundle_extract.accumulateBytes(0, std.math.maxInt(u64)));
    }
    // A single entry already past the cap → TooLarge before any accumulate.
    try std.testing.expectError(error.TooLarge, bundle_extract.accumulateBytes(0, MAX_BUNDLE_TAR_BYTES + 1));
    // A near-usize-max running total + a large entry must SATURATE, not overflow-panic.
    try std.testing.expectError(error.TooLarge, bundle_extract.accumulateBytes(std.math.maxInt(usize) - 1, 8));
    // Cumulative breach across two in-range entries → TooLarge.
    const half: u64 = @intCast(MAX_BUNDLE_TAR_BYTES / 2 + 1);
    const after_first = try bundle_extract.accumulateBytes(0, half);
    try std.testing.expectError(error.TooLarge, bundle_extract.accumulateBytes(after_first, half));
    // A valid in-range fold returns the running total unchanged in shape.
    try std.testing.expectEqual(@as(usize, 100), try bundle_extract.accumulateBytes(40, 60));
}
