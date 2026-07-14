//! Unit tests for Fleet Bundle import-request resolution (`resolve.zig`).
//! Covers the pure paths — upload validation and source-ref parsing — that do
//! not touch the network. The fetch path (github/template) needs GitHub and is
//! exercised by the github_source extraction tests + handler integration tests.

const std = @import("std");
const testing = std.testing;

const resolve = @import("resolve.zig");
const importer = @import("../../../fleet_library/importer.zig");

const SKILL = "---\nname: x\ndescription: d\nversion: 0.1.0\n---\nbody";

test "resolveUpload accepts skill-only, no heap owned" {
    var ok = try resolve.resolveUpload(.{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "paste",
        .skill_markdown = SKILL,
    });
    defer ok.deinit(testing.allocator); // no-op for uploads; proves zero ownership
    try testing.expect(ok.fetched == null);
    try testing.expectEqualStrings(SKILL, ok.body.skill_markdown);
    try testing.expectEqual(@as(usize, 0), ok.body.support_files.len);
    try testing.expectEqualStrings(importer.SOURCE_KIND_UPLOAD, ok.body.source_kind);
}

test "resolveUpload rejects missing skill" {
    try testing.expectError(resolve.Error.MissingSkill, resolve.resolveUpload(.{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
    }));
}

test "resolveUpload rejects support files (attachments are fetch-only)" {
    try testing.expectError(resolve.Error.UploadAttachmentsUnsupported, resolve.resolveUpload(.{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .skill_markdown = SKILL,
        .support_files = &.{.{ .path = "README.md", .content = "x" }},
    }));
}

test "buildSource maps template id verbatim" {
    const tmpl = try resolve.buildSource(importer.SOURCE_KIND_TEMPLATE, "github-pr-reviewer", null);
    try testing.expectEqualStrings("github-pr-reviewer", tmpl.template);
}

test "buildSource parses github owner/repo at default ref" {
    const gh = try resolve.buildSource(importer.SOURCE_KIND_GITHUB, "agentsfleet/github-pr-reviewer", null);
    try testing.expectEqualStrings("agentsfleet", gh.github.owner);
    try testing.expectEqualStrings("github-pr-reviewer", gh.github.repo);
    try testing.expectEqualStrings("main", gh.github.ref);
}

test "buildSource rejects malformed github owner/repo" {
    try testing.expectError(error.InvalidSourceRef, resolve.buildSource(importer.SOURCE_KIND_GITHUB, "no-slash", null));
    try testing.expectError(error.InvalidSourceRef, resolve.buildSource(importer.SOURCE_KIND_GITHUB, "owner/repo/extra", null));
    try testing.expectError(error.InvalidSourceRef, resolve.buildSource(importer.SOURCE_KIND_GITHUB, "/repo", null));
    try testing.expectError(error.InvalidSourceRef, resolve.buildSource(importer.SOURCE_KIND_GITHUB, "owner/", null));
    try testing.expectError(error.InvalidSourceRef, resolve.buildSource(importer.SOURCE_KIND_GITHUB, "", null));
}

test "buildSource fetches at a pinned ref instead of the default branch" {
    const gh = try resolve.buildSource(importer.SOURCE_KIND_GITHUB, "agentsfleet/github-pr-reviewer", "v2.1.0");
    try testing.expectEqualStrings("v2.1.0", gh.github.ref);
}

test "buildSource rejects a pinned ref that fails the segment rules" {
    try testing.expectError(
        error.InvalidSourceRef,
        resolve.buildSource(importer.SOURCE_KIND_GITHUB, "agentsfleet/github-pr-reviewer", ".."),
    );
}

// A ref names a git revision, so it selects content only for a github source.
// A template id resolves to fixed first-party bytes and an upload carries its
// own — a ref on either would be recorded as the source of content it never
// came from, and a later repository-only repoint could reuse that stale value
// as a real fetch ref. Both doors refuse it rather than storing a lie.
test "buildSource refuses a ref on a template source — a template has no revision to select" {
    try testing.expectError(
        error.InvalidSourceRef,
        resolve.buildSource(importer.SOURCE_KIND_TEMPLATE, "github-pr-reviewer", "v2"),
    );
}

test "resolveUpload refuses a ref — pasted bytes came from no revision" {
    try testing.expectError(error.InvalidSourceRef, resolve.resolveUpload(.{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "",
        .ref = "v2",
        .skill_markdown = SKILL,
    }));
}

test "resolveUpload records no ref when none is sent" {
    const resolved = try resolve.resolveUpload(.{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "",
        .ref = null,
        .skill_markdown = SKILL,
    });
    try testing.expectEqual(@as(?[]const u8, null), resolved.body.ref);
}
