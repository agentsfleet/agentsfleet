const std = @import("std");

const fleet_config = @import("../fleet_runtime/config.zig");
const markdown_limits = @import("../fleet_runtime/markdown_limits.zig");

pub const SOURCE_KIND_TEMPLATE = "template";
pub const SOURCE_KIND_UPLOAD = "upload";
pub const SOURCE_KIND_GITHUB = "github";
pub const VISIBILITY_WORKSPACE = "workspace";
pub const STATUS_VALID = "valid";

/// Content-addressed object-storage key layout for a bundle's canonical tar:
/// `fleet-bundles/sha256/{content_hash}.tar`. Single-sourced here so the import
/// writer (`prepare`) and the runner download handler derive the same key from a
/// content hash (RULE UFS).
pub const SNAPSHOT_KEY_PREFIX = "fleet-bundles/sha256/";
pub const SNAPSHOT_KEY_SUFFIX = ".tar";

/// Build the object-storage key for a bundle's canonical tar from its content
/// hash. Caller owns the returned slice.
pub fn snapshotKey(alloc: std.mem.Allocator, content_hash: []const u8) std.mem.Allocator.Error![]const u8 {
    return std.fmt.allocPrint(alloc, SNAPSHOT_KEY_PREFIX ++ "{s}" ++ SNAPSHOT_KEY_SUFFIX, .{content_hash});
}

const MAX_SOURCE_REF_LEN: usize = 512;
const MAX_SUPPORT_FILES: usize = 32;
const MAX_SUPPORT_PATH_LEN: usize = 160;
const MAX_SUPPORT_FILE_LEN: usize = 64 * 1024;
const MAX_SUPPORT_TOTAL_LEN: usize = 256 * 1024;

pub const ImportError = error{
    InvalidSourceKind,
    MissingSkill,
    InvalidSkill,
    InvalidTrigger,
    NameMismatch,
    UnsafePath,
    TooLarge,
    SecretShape,
};

pub const SupportFile = struct {
    path: []const u8,
    content: []const u8,
};

/// Manifest entry for a support file — path, byte size, and per-file SHA-256.
/// This is the shape persisted in Postgres `support_files_json` (the canonical
/// bytes live in R2 only), so the bundle-detail handler parses rows back into it.
pub const SupportFileManifest = struct {
    path: []const u8,
    size_bytes: usize,
    sha256: []const u8,
};

pub const ImportBody = struct {
    source_kind: []const u8,
    source_ref: []const u8 = "",
    /// The branch/tag a github source was fetched at; null ⇒ default branch.
    /// Carried so the store records the ref the content actually came from.
    ref: ?[]const u8 = null,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8 = null,
    support_files: []const SupportFile = &.{},
};

pub const PreparedBundle = struct {
    name: []const u8,
    /// SKILL.md frontmatter description — used by the platform template catalog
    /// row; the per-workspace bundle insert ignores it.
    description: []const u8,
    content_hash: []const u8,
    snapshot_key: []const u8,
    support_files_json: []const u8,
    requirements_json: []const u8,

    pub fn deinit(self: *const PreparedBundle, alloc: std.mem.Allocator) void {
        alloc.free(self.name);
        alloc.free(self.description);
        alloc.free(self.content_hash);
        alloc.free(self.snapshot_key);
        alloc.free(self.support_files_json);
        alloc.free(self.requirements_json);
    }
};

const Requirements = struct {
    credentials: []const []const u8,
    tools: []const []const u8,
    network_hosts: []const []const u8,
    support_files: []const []const u8,
    trigger_present: bool,
};

pub fn prepare(alloc: std.mem.Allocator, body: ImportBody) (std.mem.Allocator.Error || ImportError)!PreparedBundle {
    if (!validSourceKind(body.source_kind)) return ImportError.InvalidSourceKind;
    if (body.source_ref.len > MAX_SOURCE_REF_LEN) return ImportError.TooLarge;
    if (body.skill_markdown.len == 0) return ImportError.MissingSkill;
    if (body.skill_markdown.len > markdown_limits.MAX_SOURCE_LEN) return ImportError.TooLarge;
    if (body.trigger_markdown) |tm| if (tm.len == 0 or tm.len > markdown_limits.MAX_TRIGGER_LEN) return ImportError.TooLarge;
    try validateSupportFiles(body.support_files);

    var skill = fleet_config.parseSkillMetadata(alloc, body.skill_markdown) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return ImportError.InvalidSkill,
    };
    defer skill.deinit(alloc);

    const requirements_json = try buildRequirementsJson(alloc, body, skill.name);
    errdefer alloc.free(requirements_json);

    // Single pass over the support files yields both the per-file manifest hashes
    // and the whole-bundle content hash, so each support-file body is walked once
    // (Indy: fold the double hash). The bundle hash stays byte-identical to the
    // legacy order (skill, trigger, then path+content per file) — Dimension 1.2.
    const hashed = try hashBundle(alloc, body);
    const content_hash = hashed.content_hash;
    errdefer alloc.free(content_hash);
    defer {
        for (hashed.manifest) |entry| alloc.free(entry.sha256);
        alloc.free(hashed.manifest);
    }
    const support_files_json = std.json.Stringify.valueAlloc(alloc, hashed.manifest, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
    };
    errdefer alloc.free(support_files_json);

    const snapshot_key = try snapshotKey(alloc, content_hash);
    errdefer alloc.free(snapshot_key);

    // Dupe name + description out of `skill` before its defer frees it. Two
    // dupes, so build them with errdefers ahead of the literal rather than
    // inline (struct-init partial-leak rule).
    const name = try alloc.dupe(u8, skill.name);
    errdefer alloc.free(name);
    const description = try alloc.dupe(u8, skill.description);
    errdefer alloc.free(description);

    return .{
        .name = name,
        .description = description,
        .content_hash = content_hash,
        .snapshot_key = snapshot_key,
        .support_files_json = support_files_json,
        .requirements_json = requirements_json,
    };
}

pub fn validSourceKind(kind: []const u8) bool {
    return std.mem.eql(u8, kind, SOURCE_KIND_TEMPLATE) or
        std.mem.eql(u8, kind, SOURCE_KIND_UPLOAD) or
        std.mem.eql(u8, kind, SOURCE_KIND_GITHUB);
}

fn buildRequirementsJson(
    alloc: std.mem.Allocator,
    body: ImportBody,
    skill_name: []const u8,
) (std.mem.Allocator.Error || ImportError)![]const u8 {
    const support_paths = try supportFilePaths(alloc, body.support_files);
    defer alloc.free(support_paths);

    if (body.trigger_markdown) |tm| {
        var parsed = fleet_config.parseTriggerMarkdownWithJson(alloc, tm) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            else => return ImportError.InvalidTrigger,
        };
        defer parsed.deinit(alloc);
        if (!std.mem.eql(u8, skill_name, parsed.config.name)) return ImportError.NameMismatch;
        const hosts = if (parsed.config.network) |net| net.allow else &.{};
        return std.json.Stringify.valueAlloc(alloc, Requirements{
            .credentials = parsed.config.credentials,
            .tools = parsed.config.tools,
            .network_hosts = hosts,
            .support_files = support_paths,
            .trigger_present = true,
        }, .{});
    }

    return std.json.Stringify.valueAlloc(alloc, Requirements{
        .credentials = &.{},
        .tools = &.{},
        .network_hosts = &.{},
        .support_files = support_paths,
        .trigger_present = false,
    }, .{});
}

fn supportFilePaths(alloc: std.mem.Allocator, files: []const SupportFile) ![]const []const u8 {
    const paths = try alloc.alloc([]const u8, files.len);
    for (files, 0..) |file, i| paths[i] = file.path;
    return paths;
}

fn validateSupportFiles(files: []const SupportFile) ImportError!void {
    if (files.len > MAX_SUPPORT_FILES) return ImportError.TooLarge;
    var total: usize = 0;
    for (files) |file| {
        if (!safeSupportPath(file.path)) return ImportError.UnsafePath;
        if (file.content.len > MAX_SUPPORT_FILE_LEN) return ImportError.TooLarge;
        total += file.content.len;
        if (total > MAX_SUPPORT_TOTAL_LEN) return ImportError.TooLarge;
        if (containsCredentialShape(file.content)) return ImportError.SecretShape;
    }
}

fn safeSupportPath(path: []const u8) bool {
    if (path.len == 0 or path.len > MAX_SUPPORT_PATH_LEN) return false;
    if (path[0] == '/' or std.mem.indexOfScalar(u8, path, '\\') != null) return false;
    if (std.mem.eql(u8, path, "SKILL.md") or std.mem.eql(u8, path, "TRIGGER.md")) return false;

    var it = std.mem.splitScalar(u8, path, '/');
    while (it.next()) |segment| {
        if (segment.len == 0) return false;
        if (std.mem.eql(u8, segment, ".") or std.mem.eql(u8, segment, "..")) return false;
    }
    return true;
}

fn containsCredentialShape(content: []const u8) bool {
    const markers = [_][]const u8{
        "op://",
        "BEGIN PRIVATE KEY",
        "api_key:",
        "access_token:",
        "client_secret:",
        "webhook_secret:",
    };
    for (markers) |marker| {
        if (std.mem.indexOf(u8, content, marker) != null) return true;
    }
    return false;
}

const HashedBundle = struct {
    /// Whole-bundle content hash (64-char lowercase SHA-256 hex). Caller owns it.
    content_hash: []const u8,
    /// Per-support-file manifest; each `sha256` is caller-owned.
    manifest: []SupportFileManifest,
};

/// Hash the bundle and its support files in a single pass: each support-file body
/// is fed once into its own per-file hasher (manifest `sha256`) and once into the
/// rolling whole-bundle hasher (content identity), so the file list is walked once
/// instead of twice. The bundle hash covers CONTENT only (skill + trigger +
/// support paths/bytes), never source_kind/source_ref, so the same bundle imported
/// via template, github, or paste dedupes to one snapshot; source_ref stays a
/// metadata column. Caller owns `content_hash` and every `manifest[i].sha256`.
fn hashBundle(alloc: std.mem.Allocator, body: ImportBody) !HashedBundle {
    const Sha256 = std.crypto.hash.sha2.Sha256;
    var bundle = Sha256.init(.{});
    bundle.update(body.skill_markdown);
    bundle.update(&.{0});
    if (body.trigger_markdown) |tm| bundle.update(tm);
    bundle.update(&.{0});

    const manifest = try alloc.alloc(SupportFileManifest, body.support_files.len);
    var built: usize = 0;
    errdefer {
        for (manifest[0..built]) |entry| alloc.free(entry.sha256);
        alloc.free(manifest);
    }
    for (body.support_files, 0..) |file, i| {
        bundle.update(file.path);
        bundle.update(&.{0});
        bundle.update(file.content);
        bundle.update(&.{0});

        var file_hasher = Sha256.init(.{});
        file_hasher.update(file.content);
        var file_digest: [Sha256.digest_length]u8 = undefined;
        file_hasher.final(&file_digest);
        const file_hex = std.fmt.bytesToHex(file_digest, .lower);
        manifest[i] = .{
            .path = file.path,
            .size_bytes = file.content.len,
            .sha256 = try alloc.dupe(u8, file_hex[0..]),
        };
        built += 1;
    }

    var digest: [Sha256.digest_length]u8 = undefined;
    bundle.final(&digest);
    const hex = std.fmt.bytesToHex(digest, .lower);
    return .{
        .content_hash = try alloc.dupe(u8, hex[0..]),
        .manifest = manifest,
    };
}

test "prepare rejects unsafe support paths" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = SOURCE_KIND_UPLOAD,
        .skill_markdown = "---\nname: bad-path\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "../secret.txt", .content = "x" }},
    };
    try std.testing.expectError(ImportError.UnsafePath, prepare(alloc, body));
}

test "prepare lists trigger requirements" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = SOURCE_KIND_UPLOAD,
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
    const prepared = try prepare(alloc, body);
    defer prepared.deinit(alloc);
    try std.testing.expect(std.mem.indexOf(u8, prepared.requirements_json, "\"github\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.requirements_json, "api.github.com") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "README.md") != null);
}

test "prepare stores support file manifest not content" {
    const alloc = std.testing.allocator;
    const body = ImportBody{
        .source_kind = SOURCE_KIND_UPLOAD,
        .source_ref = "unit",
        .skill_markdown = "---\nname: manifest-test\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "README.md", .content = "review notes" }},
    };
    const prepared = try prepare(alloc, body);
    defer prepared.deinit(alloc);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "README.md") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "size_bytes") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "sha256") != null);
    try std.testing.expect(std.mem.indexOf(u8, prepared.support_files_json, "review notes") == null);
}
