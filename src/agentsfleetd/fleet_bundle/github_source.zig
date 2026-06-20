//! GitHub source fetch + safe extraction for Fleet Bundle import.
//!
//! `FetchedBundle` is the import-time snapshot of a `template`/`github` source:
//! SKILL.md (required) + optional TRIGGER.md + support files, all extracted and
//! validated here. The daemon re-tars the validated files into a canonical bundle
//! (`canonicalTar`) stored immutably in Cloudflare R2 and untarred verbatim by the
//! runner — the runner never sees the raw upstream archive, so this file is the
//! single trust boundary for the extracted content. It owns the extraction-side
//! guards of the security triad (the Server-Side Request Forgery guard lives in
//! `github_net.zig`):
//!   * Decompression bomb — `gunzipCapped` streams the gzip through a hard
//!     total-byte ceiling (`MAX_DECOMPRESSED_TOTAL`), so a small archive cannot
//!     expand without bound.
//!   * Path traversal — every tar entry is rejected if it is a symlink, is
//!     absolute, contains a ".." segment, or carries a backslash/NUL; only
//!     regular files under the stripped wrapper directory are kept, and dotfile
//!     entries (`.github/`, `.gitignore`) are skipped as non-bundle content.
//! `fromTarball` is pure (bytes in, `FetchedBundle` out) so the whole extraction
//! triad is exercised by `github_source_test` with crafted archives — the
//! network seam (`fetch`) is the only part that needs GitHub.

const FetchedBundle = @This();

alloc: std.mem.Allocator,
skill_markdown: []const u8,
trigger_markdown: ?[]const u8,
support_files: []SupportFile,

pub const SupportFile = struct {
    path: []const u8,
    content: []const u8,
};

/// Selects the tarball URL shape. Caller-controlled segments are charset-
/// validated before they reach a URL so they cannot inject a host or escape the
/// request path.
pub const Source = union(enum) {
    /// First-party template: repo `agentsfleet/<id>` at ref `main`; repo root is
    /// the bundle (no subpath filter).
    template: []const u8,
    /// Public GitHub repo `<owner>/<repo>` at `<ref>`.
    github: struct { owner: []const u8, repo: []const u8, ref: []const u8 },
};

pub const Error = github_net.NetError || error{
    InvalidSource,
    CorruptArchive,
    UnsafePath,
    MissingSkill,
    TooManyFiles,
};

const SKILL_NAME = "SKILL.md";
const TRIGGER_NAME = "TRIGGER.md";
const TEMPLATE_OWNER = "agentsfleet";
const TEMPLATE_REF = "main";
const TARBALL_URL_FMT = "https://api.github.com/repos/{s}/{s}/tarball/{s}";
const MAX_DECOMPRESSED_TOTAL: usize = 16 * 1024 * 1024; // 16 MiB decompression-bomb ceiling
const MAX_TAR_ENTRIES: usize = 4096; // denial-of-service guard on entry count
const MAX_SEGMENT_LEN: usize = 100; // GitHub owner/repo/ref segment ceiling
const CURRENT_DIR = ".";
const PARENT_DIR = "..";

/// Fetch + extract a template/github source. Network + decompress + untar all
/// happen here; on success the caller owns the result and must call `deinit`.
pub fn fetch(alloc: std.mem.Allocator, io: std.Io, source: Source) (Error || std.mem.Allocator.Error)!FetchedBundle {
    const url = try resolveUrl(alloc, source);
    defer alloc.free(url);
    const tar_gz = try github_net.download(alloc, io, url);
    defer alloc.free(tar_gz);
    return fromTarball(alloc, tar_gz);
}

/// Pure extraction core: gunzip (bomb-capped) → untar (traversal/symlink-safe) →
/// collect SKILL.md (required), TRIGGER.md (optional), and support files. Dupes
/// `tar_gz` into the result so the caller keeps ownership of its input.
pub fn fromTarball(alloc: std.mem.Allocator, tar_gz: []const u8) (Error || std.mem.Allocator.Error)!FetchedBundle {
    if (tar_gz.len == 0 or tar_gz.len > github_net.MAX_COMPRESSED_TARBALL) return Error.TarballTooLarge;
    const tar_bytes = try gunzipCapped(alloc, tar_gz);
    defer alloc.free(tar_bytes);
    return fromTarBytes(alloc, tar_bytes);
}

/// Extract + validate an already-decompressed tar (the gzip layer is `fromTarball`).
/// Runs the traversal/symlink/caps guards and collects SKILL.md (required),
/// TRIGGER.md (optional), and support files. Used directly by tests.
pub fn fromTarBytes(alloc: std.mem.Allocator, tar_bytes: []const u8) (Error || std.mem.Allocator.Error)!FetchedBundle {
    var accum: Accum = .{};
    errdefer accum.deinit(alloc);
    try extractInto(alloc, tar_bytes, &accum);
    const skill = accum.skill_markdown orelse return Error.MissingSkill;

    const support = try accum.support.toOwnedSlice(alloc);
    return .{
        .alloc = alloc,
        .skill_markdown = skill,
        .trigger_markdown = accum.trigger_markdown,
        .support_files = support,
    };
}

pub const CanonicalError = error{CanonicalizeFailed} || std.mem.Allocator.Error;

/// Re-tar the VALIDATED files into the canonical bundle stored in R2. The daemon
/// has already enforced the security triad on these bytes, so the result is safe
/// by construction — no wrapper directory, no symlinks, no traversal, no dotfiles
/// — and the runner untars it without re-validating. Files sit at the bundle root
/// (SKILL.md, optional TRIGGER.md, then each support path).
pub fn canonicalTar(self: *const FetchedBundle, alloc: std.mem.Allocator) CanonicalError![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    var tw: std.tar.Writer = .{ .underlying_writer = &aw.writer };
    try writeCanonicalEntry(&tw, SKILL_NAME, self.skill_markdown);
    if (self.trigger_markdown) |t| try writeCanonicalEntry(&tw, TRIGGER_NAME, t);
    for (self.support_files) |f| try writeCanonicalEntry(&tw, f.path, f.content);
    aw.writer.flush() catch return CanonicalError.OutOfMemory;
    return aw.toOwnedSlice();
}

fn writeCanonicalEntry(tw: *std.tar.Writer, path: []const u8, content: []const u8) CanonicalError!void {
    tw.writeFileBytes(path, content, .{}) catch |err| return switch (err) {
        error.WriteFailed => CanonicalError.OutOfMemory, // Allocating writer fails only on OOM
        error.OctalOverflow, error.NameTooLong => CanonicalError.CanonicalizeFailed,
    };
}

pub fn deinit(self: *FetchedBundle) void {
    self.alloc.free(self.skill_markdown);
    if (self.trigger_markdown) |t| self.alloc.free(t);
    for (self.support_files) |f| {
        self.alloc.free(f.path);
        self.alloc.free(f.content);
    }
    self.alloc.free(self.support_files);
}

/// Mutable accumulator for one extraction pass. Owns every duped slice until
/// `fromTarball` transfers them into the returned `FetchedBundle`; `deinit`
/// frees whatever is still held on an error path.
const Accum = struct {
    skill_markdown: ?[]const u8 = null,
    trigger_markdown: ?[]const u8 = null,
    support: std.ArrayList(SupportFile) = .empty,
    total_kept: usize = 0,

    fn deinit(self: *Accum, alloc: std.mem.Allocator) void {
        if (self.skill_markdown) |s| alloc.free(s);
        if (self.trigger_markdown) |t| alloc.free(t);
        for (self.support.items) |f| {
            alloc.free(f.path);
            alloc.free(f.content);
        }
        self.support.deinit(alloc);
    }
};

fn resolveUrl(alloc: std.mem.Allocator, source: Source) (Error || std.mem.Allocator.Error)![]u8 {
    switch (source) {
        .template => |id| {
            if (!validSegment(id)) return Error.InvalidSource;
            return std.fmt.allocPrint(alloc, TARBALL_URL_FMT, .{ TEMPLATE_OWNER, id, TEMPLATE_REF });
        },
        .github => |g| {
            if (!validSegment(g.owner) or !validSegment(g.repo) or !validSegment(g.ref)) return Error.InvalidSource;
            return std.fmt.allocPrint(alloc, TARBALL_URL_FMT, .{ g.owner, g.repo, g.ref });
        },
    }
}

/// A single URL path segment: non-empty, length-capped, charset
/// `[A-Za-z0-9._-]`, never "." or "..". Rejecting '/' and ':' means the segment
/// cannot inject a host or escape the path. Pure; unit-tested.
pub fn validSegment(s: []const u8) bool {
    if (s.len == 0 or s.len > MAX_SEGMENT_LEN) return false;
    if (std.mem.eql(u8, s, CURRENT_DIR) or std.mem.eql(u8, s, PARENT_DIR)) return false;
    for (s) |c| {
        if (!(std.ascii.isAlphanumeric(c) or c == '-' or c == '_' or c == '.')) return false;
    }
    return true;
}

fn gunzipCapped(alloc: std.mem.Allocator, tar_gz: []const u8) (Error || std.mem.Allocator.Error)![]u8 {
    var in = std.Io.Reader.fixed(tar_gz);
    const window = try alloc.alloc(u8, flate.max_window_len);
    defer alloc.free(window);
    var dc = flate.Decompress.init(&in, .gzip, window);
    const out = github_net.drainCapped(alloc, &dc.reader, MAX_DECOMPRESSED_TOTAL) catch |e| switch (e) {
        error.TooLarge => return Error.TarballTooLarge,
        error.ReadFailed => return Error.CorruptArchive,
        error.OutOfMemory => return error.OutOfMemory,
    };
    errdefer alloc.free(out);
    // Single member only: flate stops at gzip member 1's footer and silently
    // ignores trailing members, so reject leftover input — a multi-member archive
    // must not pass validation on member 1 alone. GitHub tarballs are single-member.
    if (in.bufferedLen() != 0) return Error.CorruptArchive;
    return out;
}

fn extractInto(alloc: std.mem.Allocator, tar_bytes: []const u8, accum: *Accum) (Error || std.mem.Allocator.Error)!void {
    var in = std.Io.Reader.fixed(tar_bytes);
    var name_buf: [std.fs.max_path_bytes]u8 = undefined;
    var link_buf: [std.fs.max_path_bytes]u8 = undefined;
    var iter = std.tar.Iterator.init(&in, .{ .file_name_buffer = &name_buf, .link_name_buffer = &link_buf });
    var entries: usize = 0;
    while (iter.next() catch return Error.CorruptArchive) |entry| {
        entries += 1;
        if (entries > MAX_TAR_ENTRIES) return Error.TooManyFiles;
        if (entry.kind == .sym_link) return Error.UnsafePath;
        if (entry.kind != .file) continue;
        try handleFile(alloc, &iter, entry, accum);
    }
}

fn handleFile(alloc: std.mem.Allocator, iter: *std.tar.Iterator, entry: std.tar.Iterator.File, accum: *Accum) (Error || std.mem.Allocator.Error)!void {
    if (!safeName(entry.name)) return Error.UnsafePath;
    const rel = stripWrapper(entry.name) orelse return; // the wrapper dir entry itself
    if (rel.len == 0 or isDotPath(rel)) return; // skip dotfiles / non-bundle content
    if (entry.size > markdown_limits.MAX_SOURCE_LEN) return Error.TarballTooLarge;
    accum.total_kept += @intCast(entry.size);
    if (accum.total_kept > MAX_DECOMPRESSED_TOTAL) return Error.TarballTooLarge;

    const content = try readEntry(alloc, iter, entry);
    errdefer alloc.free(content);

    if (std.mem.eql(u8, rel, SKILL_NAME)) {
        if (accum.skill_markdown != null) return Error.CorruptArchive;
        accum.skill_markdown = content;
    } else if (std.mem.eql(u8, rel, TRIGGER_NAME)) {
        if (accum.trigger_markdown != null) return Error.CorruptArchive;
        accum.trigger_markdown = content;
    } else {
        const path = try alloc.dupe(u8, rel);
        errdefer alloc.free(path);
        try accum.support.append(alloc, .{ .path = path, .content = content });
    }
}

fn readEntry(alloc: std.mem.Allocator, iter: *std.tar.Iterator, entry: std.tar.Iterator.File) (Error || std.mem.Allocator.Error)![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    iter.streamRemaining(entry, &aw.writer) catch return Error.CorruptArchive;
    return aw.toOwnedSlice();
}

/// Strip the single GitHub tarball wrapper directory (`<owner>-<repo>-<sha>/`).
/// Returns the remainder, or null for the wrapper directory entry itself.
fn stripWrapper(name: []const u8) ?[]const u8 {
    const slash = std.mem.indexOfScalar(u8, name, '/') orelse return null;
    const rest = name[slash + 1 ..];
    return if (rest.len == 0) null else rest;
}

/// Reject absolute paths, backslashes, NUL bytes, and any ".." segment. Run on
/// the raw tar entry name before the wrapper is stripped. Pure; unit-tested.
fn safeName(name: []const u8) bool {
    if (name.len == 0 or name[0] == '/') return false;
    if (std.mem.indexOfScalar(u8, name, '\\') != null) return false;
    if (std.mem.indexOfScalar(u8, name, 0) != null) return false;
    var it = std.mem.splitScalar(u8, name, '/');
    while (it.next()) |seg| {
        if (seg.len == 0) return false; // empty segment: leading/trailing/double slash
        if (std.mem.eql(u8, seg, PARENT_DIR)) return false;
    }
    return true;
}

fn isDotPath(rel: []const u8) bool {
    var it = std.mem.splitScalar(u8, rel, '/');
    while (it.next()) |seg| {
        if (seg.len > 0 and seg[0] == '.') return true;
    }
    return false;
}

const std = @import("std");
const flate = std.compress.flate;
const github_net = @import("github_net.zig");
const markdown_limits = @import("../fleet_runtime/markdown_limits.zig");

test {
    _ = @import("github_source_test.zig");
}
