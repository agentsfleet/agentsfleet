//! Unit tests for the Fleet Bundle GitHub extraction core (`github_source.zig`).
//! The security-critical guards — path traversal, symlink reject, caps, wrapper
//! strip, dotfile skip — run against crafted in-memory archives, plus one
//! embedded real `.tar.gz` that validates the gzip + tar path end to end. The
//! network seam (`fetch`) needs GitHub and is covered by integration + the
//! adversarial red-team, not here.

const std = @import("std");
const testing = std.testing;
const github_source = @import("github_source.zig");
const markdown_limits = @import("../fleet_runtime/markdown_limits.zig");

const SAMPLE_TARGZ = @embedFile("sample_with_folders.tar.gz");

const TarEntry = struct {
    name: []const u8,
    content: []const u8 = "",
    symlink_to: ?[]const u8 = null,
};

// Build a raw (un-gzipped) tar archive in memory from `entries`. tar(1) sanitizes
// "../" and refuses to store many malicious shapes, so we synthesize them here to
// exercise the extraction guards directly.
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

test "fromTarball extracts a real gzip fixture, strips wrapper, skips dotfiles" {
    const alloc = testing.allocator;
    var bundle = try github_source.fromTarball(alloc, SAMPLE_TARGZ);
    defer bundle.deinit();

    try testing.expectEqualStrings("skill body\n", bundle.skill_markdown);
    try testing.expect(bundle.trigger_markdown != null);
    try testing.expectEqualStrings("trigger body\n", bundle.trigger_markdown.?);
    // .gitignore is a dotfile → skipped; only scripts/run.sh remains a support file.
    try testing.expectEqual(@as(usize, 1), bundle.support_files.len);
    try testing.expectEqualStrings("scripts/run.sh", bundle.support_files[0].path);
    try testing.expectEqualStrings("echo hi\n", bundle.support_files[0].content);
}

test "fromTarBytes happy path strips the wrapper directory" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = "s" },
        .{ .name = "wrap/TRIGGER.md", .content = "t" },
        .{ .name = "wrap/docs/notes.md", .content = "n" },
    });
    defer alloc.free(tar);
    var bundle = try github_source.fromTarBytes(alloc, tar);
    defer bundle.deinit();
    try testing.expectEqualStrings("s", bundle.skill_markdown);
    try testing.expectEqualStrings("t", bundle.trigger_markdown.?);
    try testing.expectEqual(@as(usize, 1), bundle.support_files.len);
    try testing.expectEqualStrings("docs/notes.md", bundle.support_files[0].path);
}

test "fromTarBytes rejects path traversal" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = "s" },
        .{ .name = "wrap/../../etc/passwd", .content = "x" },
    });
    defer alloc.free(tar);
    try testing.expectError(error.UnsafePath, github_source.fromTarBytes(alloc, tar));
}

test "fromTarBytes rejects symlink entries" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = "s" },
        .{ .name = "wrap/link", .symlink_to = "/etc/passwd" },
    });
    defer alloc.free(tar);
    try testing.expectError(error.UnsafePath, github_source.fromTarBytes(alloc, tar));
}

test "fromTarBytes requires SKILL.md" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/TRIGGER.md", .content = "t" },
        .{ .name = "wrap/scripts/run.sh", .content = "echo" },
    });
    defer alloc.free(tar);
    try testing.expectError(error.MissingSkill, github_source.fromTarBytes(alloc, tar));
}

test "fromTarBytes rejects an over-cap file" {
    const alloc = testing.allocator;
    const big = try alloc.alloc(u8, markdown_limits.MAX_SOURCE_LEN + 1);
    defer alloc.free(big);
    @memset(big, 'x');
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = big },
    });
    defer alloc.free(tar);
    try testing.expectError(error.TarballTooLarge, github_source.fromTarBytes(alloc, tar));
}

test "validSegment accepts safe segments and rejects injection attempts" {
    try testing.expect(github_source.validSegment("github-pr-reviewer"));
    try testing.expect(github_source.validSegment("main"));
    try testing.expect(github_source.validSegment("a.b_c-1"));
    try testing.expect(!github_source.validSegment(""));
    try testing.expect(!github_source.validSegment("."));
    try testing.expect(!github_source.validSegment(".."));
    try testing.expect(!github_source.validSegment("a/b"));
    try testing.expect(!github_source.validSegment("a b"));
    try testing.expect(!github_source.validSegment("a:b"));
    try testing.expect(!github_source.validSegment("a@b"));
}

test "fromTarball rejects a multi-member gzip (only member 1 is validated)" {
    const alloc = testing.allocator;
    // Two valid single-member gzips concatenated = a 2-member gzip. flate stops at
    // member 1, so member 2 would smuggle unvalidated content; the trailing-bytes
    // guard must reject it.
    const two = try std.mem.concat(alloc, u8, &.{ SAMPLE_TARGZ, SAMPLE_TARGZ });
    defer alloc.free(two);
    try testing.expectError(error.CorruptArchive, github_source.fromTarball(alloc, two));
}

test "fromTarBytes rejects empty path segments after wrapper strip" {
    const alloc = testing.allocator;
    // 'wrap//etc/passwd' survives a pre-strip leading-slash check but strips to an
    // absolute '/etc/passwd'; the empty-segment guard rejects it.
    const tar = try buildTar(alloc, &.{.{ .name = "wrap//etc/passwd", .content = "x" }});
    defer alloc.free(tar);
    try testing.expectError(error.UnsafePath, github_source.fromTarBytes(alloc, tar));
}

test "fromTarBytes rejects a duplicate SKILL.md" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = "a" },
        .{ .name = "wrap/SKILL.md", .content = "b" },
    });
    defer alloc.free(tar);
    try testing.expectError(error.CorruptArchive, github_source.fromTarBytes(alloc, tar));
}

test "canonicalTar re-tars validated files at the bundle root (no wrapper)" {
    const alloc = testing.allocator;
    const tar = try buildTar(alloc, &.{
        .{ .name = "wrap/SKILL.md", .content = "s" },
        .{ .name = "wrap/TRIGGER.md", .content = "t" },
        .{ .name = "wrap/docs/n.md", .content = "n" },
    });
    defer alloc.free(tar);
    var bundle = try github_source.fromTarBytes(alloc, tar);
    defer bundle.deinit();

    const canon = try bundle.canonicalTar(alloc);
    defer alloc.free(canon);

    var in = std.Io.Reader.fixed(canon);
    var name_buf: [std.fs.max_path_bytes]u8 = undefined;
    var link_buf: [std.fs.max_path_bytes]u8 = undefined;
    var it = std.tar.Iterator.init(&in, .{ .file_name_buffer = &name_buf, .link_name_buffer = &link_buf });
    var saw_skill = false;
    var saw_trigger = false;
    var saw_doc = false;
    while (try it.next()) |entry| {
        if (entry.kind != .file) continue;
        if (std.mem.eql(u8, entry.name, "SKILL.md")) saw_skill = true;
        if (std.mem.eql(u8, entry.name, "TRIGGER.md")) saw_trigger = true;
        if (std.mem.eql(u8, entry.name, "docs/n.md")) saw_doc = true;
    }
    try testing.expect(saw_skill and saw_trigger and saw_doc);
}
