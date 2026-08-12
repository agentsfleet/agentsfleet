//! One-shot sweep that fills the `meta_*` projection on credentials stored
//! before `schema/036_vault_secret_metadata.sql`.
//!
//! ## Why this is a command and not a migration
//!
//! The projection is derived from the DECRYPTED credential body, and the Key
//! Encryption Key lives in the application, never in the database. A SQL
//! migration therefore cannot compute these columns — it can only add them as
//! NULL. Something holding the key has to walk the rows once.
//!
//! ## Why the read path does not do this itself
//!
//! A heal-on-read would be less operator work: notice a NULL projection while
//! serving a page, decrypt that one row, fill it in. It is rejected because it
//! puts an envelope open back on the read path, which makes "library reads never
//! decrypt" true only after every row has happened to be read once. An invariant
//! that holds eventually is not the invariant. Until this command runs, an
//! un-projected credential reports as an opaque `custom_secret` — visibly
//! incomplete rather than quietly expensive.
//!
//! ## Idempotence
//!
//! Selects only workspaces that still hold an unprojected row, and re-derives
//! the projection from the stored body rather than from anything it was told.
//! Running it twice is a no-op; running it after a partial failure resumes.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");
const crypto_store = @import("crypto_store.zig");
const metadata = @import("metadata.zig");
const sql = @import("sql.zig");

const log = logging.scoped(.metadata_backfill);

pub const Stats = struct {
    workspaces: usize = 0,
    /// Rows given a projection this run.
    projected: usize = 0,
    /// Rows whose envelope would not open. Counted and skipped rather than
    /// fatal: one damaged credential must not stop the sweep, and a row that
    /// cannot be decrypted has no projection to derive — it stays NULL and
    /// keeps reporting as opaque, which is the truthful answer.
    undecryptable: usize = 0,
    /// Rows that decrypted but whose plaintext is not a JSON object. Projected
    /// as opaque `custom_secret`, matching what `vault.storeJsonPlaintext` does
    /// for the same input on the write path.
    opaque_bodies: usize = 0,
    /// Rows that gained metadata between this sweep's decrypt and its write —
    /// i.e. were rotated underneath it. Left alone: the rotation wrote its
    /// projection in the same statement as its ciphertext, so it describes the
    /// current body and this run's projection describes a body that is gone.
    /// Counted rather than silent, because a non-zero value here is the
    /// operator's signal that the sweep raced live traffic.
    rotated_midway: usize = 0,
};

/// Walk every workspace still holding an unprojected credential and fill in the
/// four `meta_*` columns. Safe to re-run.
pub fn run(alloc: std.mem.Allocator, conn: *pg.Conn) !Stats {
    var stats: Stats = .{};

    // Collect the work list BEFORE mutating: the sweep updates the very column
    // this query filters on, so holding the cursor open while writing would be
    // reading and rewriting the same predicate at once.
    var workspaces: std.ArrayList([]const u8) = .empty;
    defer {
        for (workspaces.items) |w| alloc.free(w);
        workspaces.deinit(alloc);
    }
    {
        var q = PgQuery.from(try conn.query(sql.SELECT_WORKSPACES_NEEDING_PROJECTION, .{}));
        defer q.deinit();
        while (try q.next()) |row| {
            try workspaces.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
        }
    }

    for (workspaces.items) |workspace_id| {
        stats.workspaces += 1;
        try projectWorkspace(alloc, conn, workspace_id, &stats);
    }

    log.info("backfill.completed", .{
        .workspaces = stats.workspaces,
        .projected = stats.projected,
        .undecryptable = stats.undecryptable,
        .opaque_bodies = stats.opaque_bodies,
    });
    return stats;
}

fn projectWorkspace(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    stats: *Stats,
) !void {
    const entries = try crypto_store.loadAllForWorkspace(alloc, conn, workspace_id);
    // Zeroes every plaintext before freeing — the secret material this command
    // necessarily handles never outlives the workspace it came from.
    defer crypto_store.freeEntries(alloc, entries);

    for (entries) |entry| {
        const plaintext = entry.plaintext orelse {
            stats.undecryptable += 1;
            continue;
        };

        // Derive exactly as the write path does, so a row backfilled here and a
        // row written by `vault.storeJsonPlaintext` are indistinguishable.
        var parsed = std.json.parseFromSlice(std.json.Value, alloc, plaintext, .{}) catch {
            stats.opaque_bodies += 1;
            if (try write(conn, workspace_id, entry.key_name, .{ .kind = .custom_secret })) {
                stats.projected += 1;
            } else {
                stats.rotated_midway += 1;
            }
            continue;
        };
        defer parsed.deinit();

        if (try write(conn, workspace_id, entry.key_name, metadata.project(parsed.value))) {
            stats.projected += 1;
        } else {
            stats.rotated_midway += 1;
        }
    }
}

/// Persist one projection. Only the `meta_*` columns move — the envelope is not
/// rewritten, so a backfill cannot disturb the ciphertext it just read.
///
/// No log line here, and none anywhere in this file naming a projected value:
/// provider and base URL are non-secret but still credential metadata, which
/// section 4 keeps out of logs regardless.
/// Returns whether the row was actually filled. False means it gained metadata
/// between this sweep's decrypt and this write — a rotation — and the UPDATE's
/// `meta_kind IS NULL` guard correctly declined to overwrite fresher data with
/// this run's stale projection.
fn write(
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    projection: metadata.Projection,
) !bool {
    // The UPDATE lands only as `vault_runtime` (schema/300).
    var scope = try pool_elevation.begin(conn, .vault);
    defer scope.deinit();
    const affected = try scope.conn.exec(sql.UPDATE_SECRET_METADATA, .{
        workspace_id,
        key_name,
        projection.kind.wire(),
        projection.provider,
        projection.base_url,
        projection.has_key,
    });
    try scope.commit();
    return (affected orelse 0) > 0;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "the UPDATE refuses to overwrite a row that gained metadata mid-sweep" {
    // The sweep decrypts a whole workspace, then writes each projection some
    // time later. Without this predicate a credential rotated in that window —
    // new ciphertext AND new metadata, written atomically — would be described
    // by the projection of the plaintext this run read BEFORE the rotation.
    // The guard is in the statement rather than in the loop because a check in
    // the loop is a second read with the same gap underneath it.
    try testing.expect(std.mem.indexOf(u8, sql.UPDATE_SECRET_METADATA, "meta_kind IS NULL") != null);
}

test "the work list selects only unprojected rows, so a re-run is a no-op" {
    // Idempotence is the whole safety story for a command an operator may run
    // twice by accident, and it rests entirely on this predicate.
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_WORKSPACES_NEEDING_PROJECTION, "meta_kind IS NULL") != null);
}

test "the writer touches only projection columns, never the envelope" {
    // A backfill that rewrote ciphertext could destroy the credential it was
    // asked to describe. Pin that the statement mentions no envelope column.
    for ([_][]const u8{ "ciphertext", "encrypted_dek", "dek_nonce", "dek_tag", "nonce", "tag", "kek_version" }) |col| {
        try testing.expect(std.mem.indexOf(u8, sql.UPDATE_SECRET_METADATA, col) == null);
    }
    try testing.expect(std.mem.indexOf(u8, sql.UPDATE_SECRET_METADATA, "meta_kind") != null);
}
