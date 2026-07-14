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

const importer = @import("../../../fleet_library/importer.zig");
const FetchedBundle = @import("../../../fleet_library/github_source.zig");

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
    /// Optional branch/tag for github sources. Null fetches the default branch.
    /// The catalog's Fetch-update path sends the row's STORED ref, so a ref an
    /// operator pinned via PATCH is honored by the next fetch instead of being
    /// silently ignored and overwritten back to the default (M130).
    ref: ?[]const u8 = null,
    /// Platform add path only (M128): overwrite a catalog id that already belongs
    /// to a DIFFERENT source repository. Defaults false, so a collision is a 409
    /// the operator must acknowledge rather than a silent content swap.
    replace: bool = false,
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
    // Pasted bytes came from no revision, so there is no ref to record. Storing
    // one would leave a value a later repository-only repoint could reuse as a
    // fetch ref — a ref the content never came from.
    if (req.ref != null) return Error.InvalidSourceRef;
    return .{ .body = .{
        .source_kind = req.source_kind,
        .source_ref = req.source_ref,
        .ref = null,
        .skill_markdown = skill,
        .trigger_markdown = req.trigger_markdown,
        .support_files = &.{},
    } };
}

fn resolveFetched(alloc: std.mem.Allocator, io: std.Io, req: ImportRequest) Error!Resolved {
    const source = try buildSource(req.source_kind, req.source_ref, req.ref);
    var fb = try FetchedBundle.fetch(alloc, io, source);
    errdefer fb.deinit();
    const support = try bridgeSupport(alloc, fb.support_files); // last fallible op
    return .{
        .body = .{
            .source_kind = req.source_kind,
            .source_ref = req.source_ref,
            .ref = req.ref,
            .skill_markdown = fb.skill_markdown,
            .trigger_markdown = fb.trigger_markdown,
            .support_files = support,
        },
        .fetched = fb,
    };
}

/// Map a fetched source kind + ref to a `github_source.Source`. `template` uses
/// the ref verbatim as the first-party template id; `github` parses `owner/repo`.
pub fn buildSource(source_kind: []const u8, source_ref: []const u8, ref: ?[]const u8) error{InvalidSourceRef}!FetchedBundle.Source {
    if (std.mem.eql(u8, source_kind, importer.SOURCE_KIND_TEMPLATE)) {
        // A template id selects fixed first-party bytes — no revision is
        // consulted, so a ref selects nothing. Accepting one would record a ref
        // the content never came from, and a later repository-only repoint could
        // then reuse that stale value as a real fetch ref.
        if (ref != null) return error.InvalidSourceRef;
        return .{ .template = source_ref };
    }
    // A pinned ref rides the same segment rules as owner/repo — reject it here,
    // before the fetch, with the same error class a bad repository gets.
    const fetch_ref = ref orelse GITHUB_REF_DEFAULT;
    if (!FetchedBundle.validSegment(fetch_ref)) return error.InvalidSourceRef;
    // The segment rules (charset, length, no "."/"..") live with the URL builder
    // that enforces them — asking it here means a bad repository is refused before
    // the fetch rather than during it, and means the catalog's PATCH refuses the
    // same strings this does. Both map to UZ-BUNDLE-001 (pipeline.zig).
    const owner_repo = FetchedBundle.parseOwnerRepo(source_ref) orelse return error.InvalidSourceRef;
    return .{ .github = .{ .owner = owner_repo.owner, .repo = owner_repo.repo, .ref = fetch_ref } };
}

fn bridgeSupport(alloc: std.mem.Allocator, files: []const FetchedBundle.SupportFile) std.mem.Allocator.Error![]importer.SupportFile {
    const out = try alloc.alloc(importer.SupportFile, files.len);
    for (files, 0..) |file, i| out[i] = .{ .path = file.path, .content = file.content };
    return out;
}

test {
    _ = @import("resolve_test.zig");
}
