//! GET /v1/workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id} — one Fleet
//! library entry, with the fields the gallery summary sheds (§3).
//!
//! ## The status ladder, and why 404 covers two different facts
//!
//! Unauthenticated is 401, decided by the middleware before this runs. A caller
//! who cannot reach the workspace is 403. Everything after that — the entry does
//! not exist, or it exists in ANOTHER workspace — is one `UZ-LIBRARY-007` 404.
//!
//! Collapsing those two is deliberate. Distinguishing them would answer "does
//! this id exist somewhere?" to a caller authorized for neither, which turns the
//! detail route into an enumeration oracle over every other workspace's library.
//! The tenant query is scoped by `workspace_id`, so a foreign row simply returns
//! no rows and takes the identical path an absent one does — the two are
//! indistinguishable in the code, not merely in the response.
//!
//! ## The tier is a route segment, already validated
//!
//! `route_matchers_library.zig` parses `{tier}` into the `Tier` enum and refuses
//! to match anything else, so this handler receives a closed value and never a
//! caller-supplied string it would have to treat as a table selector.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const gallery_sql = @import("../../../fleet_library/gallery_sql.zig");
const counters = @import("../../../observability/library_read_counters.zig");
const entry_view = @import("entry_view.zig");
const keyset = @import("fleet_keyset.zig");
const response_size = @import("../../response_size.zig");

const Hx = hx_mod.Hx;

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_NOT_FOUND = "No fleet library entry matches this tier and id in this workspace";
const S_DETAIL_FAILED = "Failed to load this fleet library entry";
const S_BODY_CEILING = "This fleet library entry is too large to return";

/// MUST match the write below — see the gallery's twin.
const DETAIL_JSON_OPTIONS: std.json.Stringify.Options = .{};

/// One entry in full. Superset of the gallery card: everything the summary
/// carries, plus the requirement and support-file detail the install gate needs.
///
/// As with the summary, the load-bearing property is what this CANNOT express —
/// no `skill_markdown`, no `trigger_markdown`, no support-file body, no
/// object-store key (M128 Invariant 3).
const DetailEntry = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    visibility: []const u8,
    source_ref: []const u8,
    created_at: i64,
    requirements: entry_view.Requirements,
    required_credentials_reasons: std.json.Value,
    support_files: []const entry_view.SupportSummary,
};

pub fn innerGalleryDetail(
    hx: Hx,
    workspace_id: []const u8,
    tier: keyset.Tier,
    id: []const u8,
) void {
    counters.beginRead();
    defer counters.endRead();

    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    var db = hx.db() orelse return;
    defer db.end();
    counters.noteConnection();

    if (!common.authorizeWorkspace(db.conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    const found = load(hx, db.conn, workspace_id, tier, id) catch {
        common.internalOperationError(hx.res, S_DETAIL_FAILED, hx.req_id);
        return;
    };
    const entry = found orelse {
        hx.fail(ec.ERR_LIBRARY_ENTRY_NOT_FOUND, S_NOT_FOUND);
        return;
    };
    counters.noteResults(1);
    respond(hx, entry);
}

/// Write the entry under §3's 1 MiB detail ceiling. Same refuse-never-truncate
/// contract as the gallery's `respond`; the ceiling is larger because this
/// carries the support-file manifest the card sheds.
fn respond(hx: Hx, entry: DetailEntry) void {
    const encoded_bytes = response_size.encodedWithinCeiling(
        entry,
        DETAIL_JSON_OPTIONS,
        counters.FLEET_DETAIL_MAX_BODY_BYTES,
    ) catch |err| {
        if (err == response_size.CeilingError.BodyCeilingExceeded) {
            hx.fail(ec.ERR_LIBRARY_BODY_CEILING, S_BODY_CEILING);
        } else {
            common.internalOperationError(hx.res, S_DETAIL_FAILED, hx.req_id);
        }
        return;
    };
    counters.noteEncodedBytes(encoded_bytes);
    common.writeJson(hx.res, .ok, entry);
}

fn load(
    hx: Hx,
    conn: *pg.Conn,
    workspace_id: []const u8,
    tier: keyset.Tier,
    id: []const u8,
) !?DetailEntry {
    return switch (tier) {
        .platform => loadPlatform(hx.alloc, conn, id),
        .tenant => loadTenant(hx.alloc, conn, workspace_id, id),
    };
}

fn loadPlatform(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !?DetailEntry {
    var q = PgQuery.from(try conn.query(
        gallery_sql.SELECT_GALLERY_DETAIL_PLATFORM,
        .{ id, library_store.VISIBILITY_PUBLIC },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return .{
        .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .source_ref = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .created_at = try row.get(i64, 4),
        .visibility = library_store.TIER_PLATFORM,
        .requirements = .{
            .credentials = try entry_view.decodeStrings(alloc, try row.get([]const u8, 5)),
            .tools = try entry_view.decodeStrings(alloc, try row.get([]const u8, 6)),
            .network_hosts = try entry_view.decodeStrings(alloc, try row.get([]const u8, 7)),
            .trigger_present = try row.get(bool, 10),
        },
        .required_credentials_reasons = try entry_view.decodeReasons(alloc, try row.get([]const u8, 8)),
        .support_files = try entry_view.decodeSummaries(alloc, try row.get([]const u8, 9)),
    };
}

/// Scoped by `workspace_id`, so a foreign entry returns no row and reaches the
/// caller as the same 404 an absent one does. That is the non-enumerating
/// property, and it holds because the query cannot see the other workspace at
/// all — not because the handler remembers to hide it.
fn loadTenant(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    id: []const u8,
) !?DetailEntry {
    // A malformed id can never match a row, and letting it reach the `::uuid`
    // cast would surface a client input fault as a 500.
    if (!id_format.isUuidV7(id)) return null;

    var q = PgQuery.from(try conn.query(
        gallery_sql.SELECT_GALLERY_DETAIL_TENANT,
        .{ id, workspace_id },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return .{
        .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .source_ref = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .created_at = try row.get(i64, 4),
        .visibility = library_store.TIER_TENANT,
        .requirements = try std.json.parseFromSliceLeaky(
            entry_view.Requirements,
            alloc,
            try row.get([]const u8, 5),
            .{ .ignore_unknown_fields = true },
        ),
        // The importer derives no reasons for a tenant entry, so this is the
        // empty object rather than an absent key — a consumer reading it should
        // not have to distinguish "none" from "not applicable".
        .required_credentials_reasons = try entry_view.decodeReasons(alloc, library_store.EMPTY_REASONS_JSON),
        .support_files = try entry_view.decodeSummaries(alloc, try row.get([]const u8, 6)),
    };
}
