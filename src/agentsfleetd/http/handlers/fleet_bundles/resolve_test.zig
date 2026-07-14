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

// A template has no git ref, but the value still lands in the store — the same
// segment rules apply, so a template row can never carry a ref string the
// github path would have refused.
test "buildSource rejects a malformed ref on a template source too" {
    try testing.expectError(
        error.InvalidSourceRef,
        resolve.buildSource(importer.SOURCE_KIND_TEMPLATE, "github-pr-reviewer", ".."),
    );
}

test "buildSource accepts a template with a well-formed or absent ref" {
    const pinned = try resolve.buildSource(importer.SOURCE_KIND_TEMPLATE, "github-pr-reviewer", "v1");
    try testing.expectEqualStrings("github-pr-reviewer", pinned.template);
}
