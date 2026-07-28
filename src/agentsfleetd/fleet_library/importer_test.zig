//! Behaviour tests for the import pipeline's `prepare` step: path safety,
//! trigger-derived requirements, and the support-file manifest's write-only
//! contract — built and persisted on every import, returned by no response.

const std = @import("std");
const importer = @import("importer.zig");
const sql = @import("sql.zig");

const ImportBody = importer.ImportBody;

test "prepare rejects unsafe support paths" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .skill_markdown = "---\nname: bad-path\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "../secret.txt", .content = "x" }},
    };
    try std.testing.expectError(importer.ImportError.UnsafePath, importer.prepare(alloc, body));
}

test "prepare lists trigger requirements" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit",
        .skill_markdown = "---\nname: github-pr-reviewer\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .trigger_markdown =
        \\---
        \\name: github-pr-reviewer
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: webhook
        \\      source: github
        \\  credentials: [github]
        \\  tools: [http_request]
        \\  network:
        \\    allow: [api.github.com]
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
        ,
        .support_files = &.{.{ .path = "README.md", .content = "review notes" }},
    };
    const prepared = try importer.prepare(alloc, body);
    defer prepared.deinit(alloc);
    try std.testing.expect(std.mem.indexOf(u8, prepared.requirements_json, "\"github\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.requirements_json, "api.github.com") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "README.md") != null);
}

test "prepare stores support file manifest not content" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit",
        .skill_markdown = "---\nname: manifest-test\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "README.md", .content = "review notes" }},
    };
    const prepared = try importer.prepare(alloc, body);
    defer prepared.deinit(alloc);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "README.md") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "size_bytes") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "sha256") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "review notes") == null);
}

test "test_import_still_persists_support_manifest" {
    // Dimension 5.2. `support_files` was removed from every API response, and
    // this pins the half that must NOT have moved with it: the manifest is still
    // built on import and still written.
    //
    // The two are easy to conflate — both are spelled `support_files` — so the
    // failure this guards against is a later cleanup deleting the write because
    // "nothing reads it". Nothing reads it back on purpose; it is a record of
    // what a stored bundle held, and the bytes it describes are load-bearing.
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit",
        .skill_markdown = "---\nname: retained-manifest\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "docs/NOTES.md", .content = "kept" }},
    };
    const prepared = try importer.prepare(alloc, body);
    defer prepared.deinit(alloc);

    // Built, and not the empty manifest a dropped write would leave behind.
    try std.testing.expect(!std.mem.eql(u8, prepared.support_files_json, "[]"));
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "docs/NOTES.md") != null);

    // And still carried by the statements that write it. A projection change
    // cannot silently take the column out of the INSERT.
    try std.testing.expect(std.mem.indexOf(u8, sql.INSERT_PLATFORM, "support_files_json") != null);
    try std.testing.expect(std.mem.indexOf(u8, sql.INSERT_TENANT, "support_files_json") != null);

    // The read side is the part that went: no operator-facing projection
    // selects it any more.
    try std.testing.expect(std.mem.indexOf(u8, sql.SELECT_ADMIN_CATALOG, "support_files_json") == null);
    try std.testing.expect(std.mem.indexOf(u8, sql.SELECT_ADMIN_CATALOG_ROW, "support_files_json") == null);
}
