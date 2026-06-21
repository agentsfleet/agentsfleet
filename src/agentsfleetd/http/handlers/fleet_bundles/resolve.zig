//! Resolves a Fleet Bundle import request into a validated `importer.ImportBody`.
//!
//! Two source shapes converge here:
//!   * `upload` (paste) — the request carries SKILL.md (required) and optional
//!     TRIGGER.md markdown inline; support files are NOT accepted (attachments
//!     ride only on fetched sources).
//!   * `github` / `template` — the daemon fetches the bundle from GitHub
//!     server-side (`github_source.fetch`); the fetched, extraction-validated
//!     SKILL.md / TRIGGER.md / support files become the body.
//!
//! `Resolved` owns the lifetime of fetched content (and the bridged support
//! array); the borrowed wire request must outlive it. Caller frees via `deinit`.

const std = @import("std");

const importer = @import("../../../fleet_bundle/importer.zig");
const FetchedBundle = @import("../../../fleet_bundle/github_source.zig");

/// GitHub sources resolve at the default branch ref until per-source commit
/// pinning lands.
const GITHUB_REF_DEFAULT = "main";

/// Wire shape of a Fleet Bundle import request. `skill_markdown` is present only
/// for `upload`; `source_ref` carries the template id or `owner/repo` for fetched
/// sources. `support_files` is rejected for `upload` and unused otherwise — the
/// fetch is the authoritative source of attachments.
pub const ImportRequest = struct {
    source_kind: []const u8,
    source_ref: []const u8 = "",
    skill_markdown: ?[]const u8 = null,
    trigger_markdown: ?[]const u8 = null,
    support_files: []const importer.SupportFile = &.{},
};

pub const Error = error{
    InvalidSourceKind,
    MissingSkill,
    UploadAttachmentsUnsupported,
    InvalidSourceRef,
} || FetchedBundle.Error || std.mem.Allocator.Error;

/// An import request resolved to an `importer.ImportBody`. Built by `resolve`;
/// the caller passes the same allocator to `deinit`. For fetched sources it owns
/// the `FetchedBundle` (SKILL/TRIGGER/support content) plus the bridged support
/// array; for uploads it owns nothing (the body borrows the wire request).
pub const Resolved = struct {
    body: importer.ImportBody,
    fetched: ?FetchedBundle = null,

    pub fn deinit(self: *Resolved, alloc: std.mem.Allocator) void {
        if (self.fetched) |*fb| {
            alloc.free(self.body.support_files); // bridged array; entries owned by fb
            fb.deinit();
        }
    }
};

/// Resolve a request by source kind. Upload validates inline markdown; github /
/// template fetch + extract server-side. Returns `InvalidSourceKind` otherwise.
pub fn resolve(alloc: std.mem.Allocator, io: std.Io, req: ImportRequest) Error!Resolved {
    if (std.mem.eql(u8, req.source_kind, importer.SOURCE_KIND_UPLOAD)) return resolveUpload(req);
    if (std.mem.eql(u8, req.source_kind, importer.SOURCE_KIND_TEMPLATE) or
        std.mem.eql(u8, req.source_kind, importer.SOURCE_KIND_GITHUB))
    {
        return resolveFetched(alloc, io, req);
    }
    return Error.InvalidSourceKind;
}

/// Upload (paste): SKILL.md required, support files rejected. The body borrows
/// the wire request, so `Resolved` owns no heap and `deinit` is a no-op.
pub fn resolveUpload(req: ImportRequest) Error!Resolved {
    const skill = req.skill_markdown orelse return Error.MissingSkill;
    if (req.support_files.len > 0) return Error.UploadAttachmentsUnsupported;
    return .{ .body = .{
        .source_kind = req.source_kind,
        .source_ref = req.source_ref,
        .skill_markdown = skill,
        .trigger_markdown = req.trigger_markdown,
        .support_files = &.{},
    } };
}

fn resolveFetched(alloc: std.mem.Allocator, io: std.Io, req: ImportRequest) Error!Resolved {
    const source = try buildSource(req.source_kind, req.source_ref);
    var fb = try FetchedBundle.fetch(alloc, io, source);
    errdefer fb.deinit();
    const support = try bridgeSupport(alloc, fb.support_files); // last fallible op
    return .{
        .body = .{
            .source_kind = req.source_kind,
            .source_ref = req.source_ref,
            .skill_markdown = fb.skill_markdown,
            .trigger_markdown = fb.trigger_markdown,
            .support_files = support,
        },
        .fetched = fb,
    };
}

/// Map a fetched source kind + ref to a `github_source.Source`. `template` uses
/// the ref verbatim as the first-party template id; `github` parses `owner/repo`.
pub fn buildSource(source_kind: []const u8, source_ref: []const u8) error{InvalidSourceRef}!FetchedBundle.Source {
    if (std.mem.eql(u8, source_kind, importer.SOURCE_KIND_TEMPLATE)) {
        return .{ .template = source_ref };
    }
    const owner_repo = try parseOwnerRepo(source_ref);
    return .{ .github = .{ .owner = owner_repo.owner, .repo = owner_repo.repo, .ref = GITHUB_REF_DEFAULT } };
}

fn parseOwnerRepo(source_ref: []const u8) error{InvalidSourceRef}!struct { owner: []const u8, repo: []const u8 } {
    const slash = std.mem.indexOfScalar(u8, source_ref, '/') orelse return error.InvalidSourceRef;
    const owner = source_ref[0..slash];
    const repo = source_ref[slash + 1 ..];
    if (owner.len == 0 or repo.len == 0) return error.InvalidSourceRef;
    if (std.mem.indexOfScalar(u8, repo, '/') != null) return error.InvalidSourceRef;
    return .{ .owner = owner, .repo = repo };
}

fn bridgeSupport(alloc: std.mem.Allocator, files: []const FetchedBundle.SupportFile) std.mem.Allocator.Error![]importer.SupportFile {
    const out = try alloc.alloc(importer.SupportFile, files.len);
    for (files, 0..) |file, i| out[i] = .{ .path = file.path, .content = file.content };
    return out;
}

test {
    _ = @import("resolve_test.zig");
}
