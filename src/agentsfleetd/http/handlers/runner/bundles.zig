//! Runner-plane Fleet Bundle snapshot download — the daemon proxy in front of R2.
//!
//!   GET /v1/runners/me/bundles/{content_hash} → innerRunnerBundle
//!
//! The runner holds zero datastore credentials, so it cannot reach R2 directly;
//! the daemon (which does hold the R2 keys) serves the immutable canonical tar by
//! content hash. The runner learns the hash from its lease (`LeasePayload.bundle`)
//! and materializes the support files into the sandbox.
//!
//! Auth: `runnerBearer` (`agt_r`). The bundle is content-addressed by SHA-256 and
//! carries NO secrets (resolved secret values never enter the R2 archive —
//! credentials ride the lease's secret_delivery), so an authenticated runner
//! fetching by an unguessable hash is the access boundary; the key is rebuilt
//! server-side from the validated hash so the path can never inject an R2 key.

const std = @import("std");
const logging = @import("log");

const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const hx_mod = @import("../hx.zig");
const importer = @import("../../../fleet_bundle/importer.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_bundle);

/// A content hash is the lowercase hex of a SHA-256 digest (32 bytes → 64 chars).
const SHA256_HEX_LEN: usize = 64;
const S_CONTENT_TYPE_TAR = "application/x-tar";
const S_RUNNER_IDENTITY_REQUIRED = "runner identity required";
const S_STORAGE_UNAVAILABLE = "Fleet Bundle snapshot storage is unavailable";

/// Stream the immutable canonical tar for `content_hash` from R2. 404 means the
/// bundle is skill-only (no support files were stored) and the runner proceeds
/// with none; 503 means storage is unconfigured/unavailable.
pub fn innerRunnerBundle(hx: Hx, content_hash: []const u8) void {
    if (hx.principal.runner_id == null) {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, S_RUNNER_IDENTITY_REQUIRED);
        return;
    }
    if (!isContentHash(content_hash)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "bundle ref must be a 64-character lowercase sha256 hex digest");
        return;
    }
    const r2 = hx.ctx.r2 orelse {
        common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, S_STORAGE_UNAVAILABLE, hx.req_id);
        return;
    };
    const key = importer.snapshotKey(hx.alloc, content_hash) catch {
        common.internalOperationError(hx.res, "bundle key build failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(key);
    const tar = r2.get(hx.alloc, key) catch |err| {
        switch (err) {
            error.R2NotFound => hx.fail(ec.ERR_FLEET_BUNDLE_NOT_FOUND, "no snapshot stored for this content hash"),
            else => common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, "Fleet Bundle snapshot fetch failed", hx.req_id),
        }
        return;
    };
    log.debug("runner_bundle_served", .{ .bytes = tar.len, .req_id = hx.req_id });
    hx.res.status = 200;
    hx.res.header(common.HEADER_CONTENT_TYPE, S_CONTENT_TYPE_TAR);
    hx.res.body = tar;
}

/// Lowercase SHA-256 hex: exactly 64 chars of `[0-9a-f]`. Rejecting anything else
/// keeps a caller-supplied ref from manipulating the rebuilt R2 key.
fn isContentHash(s: []const u8) bool {
    if (s.len != SHA256_HEX_LEN) return false;
    for (s) |c| {
        const is_lower_hex = (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
        if (!is_lower_hex) return false;
    }
    return true;
}

test "isContentHash accepts a 64-char lowercase sha256 and rejects everything else" {
    const ok = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    try std.testing.expect(isContentHash(ok));
    try std.testing.expect(!isContentHash("")); // empty
    try std.testing.expect(!isContentHash(ok[0..63])); // too short
    try std.testing.expect(!isContentHash(ok ++ "a")); // too long
    try std.testing.expect(!isContentHash("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855")); // uppercase
    try std.testing.expect(!isContentHash("../../etc/passwd000000000000000000000000000000000000000000000000")); // path chars
}
