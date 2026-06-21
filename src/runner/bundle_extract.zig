//! bundle_extract.zig — runner-side Fleet Bundle materialization.
//!
//! When a lease carries a `BundleManifest` (`content_hash`), the host daemon (which
//! holds the `agt_r` — the sandboxed child never does) downloads the bundle's
//! immutable canonical tar via the daemon proxy (`GET /v1/runners/me/bundles/{hash}`,
//! the shared `control_plane_client.get` primitive), caches it content-addressed
//! (download once, reuse across runs), and untars the SUPPORT FILES (and folders)
//! into the per-lease sandbox workspace BEFORE the child forks.
//!
//! Support-files-only: the canonical tar packs `SKILL.md` + optional `TRIGGER.md` +
//! support files at the bundle root (see `github_source.canonicalTar`), but the
//! lease's `instructions`/`policy` are the AUTHORITATIVE behaviour — they reflect
//! later fleet PATCHes, while the tar's import-time `SKILL.md`/`TRIGGER.md` are the
//! original copies. Materializing those would put stale behaviour on disk, so they
//! are skipped.
//!
//! Trust: the tar is daemon-validated (no symlinks/traversal/wrapper/dotfiles — safe
//! by construction), so this is NOT a trust boundary; the cheap `safeRel` re-check is
//! belt-and-suspenders against a corrupt cache file, not the primary defense (that
//! lives in `github_source.zig`).
//!
//! Caching: the raw tar is cached at `{workspace_base}/.bundle-cache/{hash}.tar`,
//! keyed by the immutable content hash (lease expiry never invalidates it) and
//! written atomically (temp + rename). Support files are re-extracted into each
//! per-lease workspace (tiny — import-capped 256 KiB) rather than RO bind-mounted,
//! so the path is tier-agnostic (`dev_none` and bwrap alike) and needs no
//! `sandbox_args` change.

const std = @import("std");
const contract = @import("contract");
const logging = @import("log");
const control_plane_client = @import("daemon/control_plane_client.zig");
const client_errors = @import("engine/client_errors.zig");

const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;

/// Canonical-tar root entries the runner must NOT materialize — the lease's
/// `instructions`/`policy` are the authoritative behaviour, so the tar's
/// import-time copies are skipped. Mirror `github_source.canonicalTar`'s names.
const SKILL_NAME = "SKILL.md";
const TRIGGER_NAME = "TRIGGER.md";

/// Cache subdir under `workspace_base` holding content-addressed canonical tars.
/// The leading dot keeps it distinct from per-lease workspace dirs (UUID names),
/// so the per-lease `deleteTree` cleanup never reaps it.
const CACHE_SUBDIR = ".bundle-cache";
const CACHE_SUFFIX = ".tar";
/// Temp file the download is written to before the atomic rename into the cache.
/// It lives in the per-lease workspace (same filesystem under `workspace_base`, so
/// the rename is atomic) and is auto-reaped by the per-lease cleanup on a crash.
const CACHE_TMP_NAME = ".bundle-cache.tmp";

/// Upper bound on a downloaded/cached canonical tar. Defense in depth over the
/// import-side caps (SKILL ≤200 KiB + TRIGGER ≤200 KiB + support ≤256 KiB + tar
/// overhead is < 1 MiB), bounding the one buffered read so a misbehaving proxy or
/// a corrupt cache file cannot balloon runner memory.
const MAX_BUNDLE_TAR_BYTES: usize = 4 * 1024 * 1024;
/// Entry-count ceiling when untarring a (trusted) tar — a corruption guard, well
/// above the import-side support-file cap.
const MAX_TAR_ENTRIES: usize = 4096;
/// `{parent}/{child}` path-join format (RULE UFS — 4 bufPrint sites).
const PATH_JOIN_FMT = "{s}/{s}";

/// Outcome of materializing a leased bundle's support files into the workspace.
pub const MaterializeResult = enum {
    /// Proceed to execute — support files are in place, or the bundle is
    /// skill-only / carried none (nothing to materialize).
    ready,
    /// Download or extraction failed — the caller reports a startup failure and
    /// does not fork the child. Retry is deferred (see spec Failure Modes).
    failed,
};

/// Materialize the leased bundle's SUPPORT FILES (and folders) into the per-lease
/// `workspace_path` before the child forks: content-addressed cache → daemon
/// download on a miss → trusted untar of support files only. `SKILL.md`/
/// `TRIGGER.md` in the tar are skipped. A `404` is a skill-only bundle and yields
/// `.ready` with nothing written.
pub fn materialize(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: *control_plane_client,
    runner_token: []const u8,
    workspace_base: []const u8,
    workspace_path: []const u8,
    manifest: protocol.BundleManifest,
    deadline_ms: u31,
) MaterializeResult {
    const tar = cacheOrDownload(io, alloc, cp, runner_token, workspace_base, workspace_path, manifest.content_hash, deadline_ms) catch |err| {
        log.warn("bundle_download_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .content_hash = manifest.content_hash, .err = @errorName(err) });
        return .failed;
    };
    const bytes = tar orelse {
        log.debug("bundle_skill_only", .{ .content_hash = manifest.content_hash });
        return .ready; // 404 — no support files were stored for this bundle
    };
    defer alloc.free(bytes);
    const written = extractSupportFiles(io, alloc, bytes, workspace_path) catch |err| {
        log.warn("bundle_extract_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .content_hash = manifest.content_hash, .err = @errorName(err) });
        return .failed;
    };
    log.debug("bundle_materialized", .{ .content_hash = manifest.content_hash, .files = written });
    return .ready;
}

/// Return the canonical tar for `content_hash`: from the content-addressed cache
/// when present, else downloaded via the daemon proxy and written through to the
/// cache (best-effort — a cache write failure never fails the run). `null` is the
/// skill-only (`404`) case. Caller owns a returned slice.
fn cacheOrDownload(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: *control_plane_client,
    runner_token: []const u8,
    workspace_base: []const u8,
    workspace_path: []const u8,
    content_hash: []const u8,
    deadline_ms: u31,
) !?[]u8 {
    if (readCache(io, alloc, workspace_base, content_hash)) |cached| {
        log.debug("bundle_cache_hit", .{ .content_hash = content_hash });
        return cached;
    }
    const tar = (try downloadBundle(cp, alloc, runner_token, content_hash, deadline_ms)) orelse return null;
    writeCache(io, workspace_base, workspace_path, content_hash, tar) catch |err|
        log.warn("bundle_cache_write_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .content_hash = content_hash, .err = @errorName(err) });
    return tar;
}

/// GET the bundle's canonical tar via the daemon proxy (`cp.get`, the shared GET
/// primitive). Returns the tar bytes (caller owns), or `null` for a skill-only
/// bundle (`404` = no support files were stored). Non-2xx or an over-cap body is
/// `BadStatus` (fail closed). The tar is daemon-validated (trusted) — untarred
/// without re-validation; one buffered attempt, retry deferred.
fn downloadBundle(cp: *control_plane_client, alloc: std.mem.Allocator, runner_token: []const u8, content_hash: []const u8, deadline_ms: u31) !?[]u8 {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = std.fmt.bufPrint(&buf, PATH_JOIN_FMT, .{ protocol.PATH_RUNNER_BUNDLES, content_hash }) catch return error.PathTooLong;
    const res = try cp.get(alloc, path, runner_token, deadline_ms);
    if (res.status == 404) {
        alloc.free(res.body);
        return null;
    }
    if (res.status < 200 or res.status >= 300 or res.body.len > MAX_BUNDLE_TAR_BYTES) {
        alloc.free(res.body);
        return control_plane_client.ClientError.BadStatus;
    }
    return res.body;
}

/// Read the cached canonical tar for `content_hash`, or `null` on any miss
/// (absent, unreadable, oversized). Errors degrade to a miss so the caller
/// re-downloads — the cache is an optimization, never a hard dependency.
/// `pub` for the sibling `bundle_extract_test.zig` cache round-trip.
pub fn readCache(io: std.Io, alloc: std.mem.Allocator, workspace_base: []const u8, content_hash: []const u8) ?[]u8 {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = cachePath(&buf, workspace_base, content_hash) catch return null;
    return std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(MAX_BUNDLE_TAR_BYTES)) catch null;
}

/// Persist `tar` to the content-addressed cache atomically: write to a per-lease
/// temp file, then rename it into `{workspace_base}/.bundle-cache/{hash}.tar`. The
/// temp lives in `workspace_path` (same filesystem → atomic rename; auto-reaped by
/// the per-lease cleanup on failure). Concurrent workers racing the same hash are
/// harmless — the bytes are identical (content-addressed). `pub` for the sibling test.
pub fn writeCache(io: std.Io, workspace_base: []const u8, workspace_path: []const u8, content_hash: []const u8, tar: []const u8) !void {
    var dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const cache_dir = try std.fmt.bufPrint(&dir_buf, PATH_JOIN_FMT, .{ workspace_base, CACHE_SUBDIR });
    std.Io.Dir.createDirAbsolute(io, cache_dir, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
    var tmp_buf: [std.fs.max_path_bytes]u8 = undefined;
    const tmp_path = try std.fmt.bufPrint(&tmp_buf, PATH_JOIN_FMT, .{ workspace_path, CACHE_TMP_NAME });
    {
        const file = try std.Io.Dir.createFileAbsolute(io, tmp_path, .{});
        defer file.close(io);
        try file.writeStreamingAll(io, tar);
    }
    var final_buf: [std.fs.max_path_bytes]u8 = undefined;
    const final_path = try cachePath(&final_buf, workspace_base, content_hash);
    try std.Io.Dir.renameAbsolute(tmp_path, final_path, io);
}

/// Build `{workspace_base}/.bundle-cache/{content_hash}.tar` into `buf`.
fn cachePath(buf: []u8, workspace_base: []const u8, content_hash: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}/{s}/{s}{s}", .{ workspace_base, CACHE_SUBDIR, content_hash, CACHE_SUFFIX });
}

/// Untar `tar_bytes` (the daemon-validated canonical bundle) into `workspace_path`,
/// materializing SUPPORT FILES ONLY — `SKILL.md`/`TRIGGER.md` are skipped, nested
/// support paths create their parent folders. Returns the count written. Rejects a
/// symlink/traversal entry (corrupt-cache guard) and caps entries + total bytes.
/// `pub` for the sibling `bundle_extract_test.zig`.
pub fn extractSupportFiles(io: std.Io, alloc: std.mem.Allocator, tar_bytes: []const u8, workspace_path: []const u8) !usize {
    var in = std.Io.Reader.fixed(tar_bytes);
    var name_buf: [std.fs.max_path_bytes]u8 = undefined;
    var link_buf: [std.fs.max_path_bytes]u8 = undefined;
    var iter = std.tar.Iterator.init(&in, .{ .file_name_buffer = &name_buf, .link_name_buffer = &link_buf });
    var entries: usize = 0;
    var written: usize = 0;
    var total: usize = 0;
    while (iter.next() catch return error.CorruptArchive) |entry| {
        entries += 1;
        if (entries > MAX_TAR_ENTRIES) return error.TooManyEntries;
        if (entry.kind == .sym_link) return error.UnsafePath;
        if (entry.kind != .file) continue;
        const rel = entry.name;
        if (std.mem.eql(u8, rel, SKILL_NAME) or std.mem.eql(u8, rel, TRIGGER_NAME)) continue;
        if (!safeRel(rel)) return error.UnsafePath;
        total += @intCast(entry.size);
        if (total > MAX_BUNDLE_TAR_BYTES) return error.TooLarge;
        try writeEntry(io, alloc, &iter, entry, workspace_path, rel);
        written += 1;
    }
    return written;
}

/// Materialize one tar entry at `{workspace_path}/{rel}`, creating parent folders.
fn writeEntry(io: std.Io, alloc: std.mem.Allocator, iter: *std.tar.Iterator, entry: std.tar.Iterator.File, workspace_path: []const u8, rel: []const u8) !void {
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const abs = try std.fmt.bufPrint(&path_buf, PATH_JOIN_FMT, .{ workspace_path, rel });
    try ensureParentDirs(io, abs, workspace_path.len);
    const content = try readEntry(alloc, iter, entry);
    defer alloc.free(content);
    const file = try std.Io.Dir.createFileAbsolute(io, abs, .{});
    defer file.close(io);
    try file.writeStreamingAll(io, content);
}

/// Stream one tar entry's bytes into an owned slice (caller frees). Entry sizes
/// are import-capped, so the transient buffer is small.
fn readEntry(alloc: std.mem.Allocator, iter: *std.tar.Iterator, entry: std.tar.Iterator.File) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    iter.streamRemaining(entry, &aw.writer) catch return error.CorruptArchive;
    return aw.toOwnedSlice();
}

/// Create every parent folder of `abs` below the workspace root (`root_len` is the
/// workspace prefix length), tolerating existing dirs. Walks the separators after
/// the root so nested support paths (`checklists/owasp.md`) get their folders.
fn ensureParentDirs(io: std.Io, abs: []const u8, root_len: usize) !void {
    var i = root_len + 1; // skip the '/' joining workspace_path and rel
    while (std.mem.indexOfScalarPos(u8, abs, i, '/')) |slash| {
        std.Io.Dir.createDirAbsolute(io, abs[0..slash], .default_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
        i = slash + 1;
    }
}

/// Belt-and-suspenders path check on a (trusted) canonical-tar entry name: reject
/// absolute paths, backslashes, NUL, and empty/`.`/`..` segments. The daemon
/// already enforced these at import; this guards a corrupt cache file.
fn safeRel(rel: []const u8) bool {
    if (rel.len == 0 or rel[0] == '/') return false;
    if (std.mem.indexOfScalar(u8, rel, '\\') != null) return false;
    if (std.mem.indexOfScalar(u8, rel, 0) != null) return false;
    var it = std.mem.splitScalar(u8, rel, '/');
    while (it.next()) |seg| {
        if (seg.len == 0) return false;
        if (std.mem.eql(u8, seg, ".") or std.mem.eql(u8, seg, "..")) return false;
    }
    return true;
}
