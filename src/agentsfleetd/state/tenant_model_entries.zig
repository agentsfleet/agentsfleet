const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");
const sql = @import("tenant_model_entries/sql.zig");

const SQLSTATE_UNIQUE_VIOLATION = "23505";

pub const StateError = error{
    DuplicateEntry,
    NotFound,
};

pub const Entry = struct {
    id: []const u8,
    tenant_id: []const u8,
    model_id: []const u8,
    secret_ref: []const u8,
    created_at: i64,
    updated_at: i64,

    pub fn deinit(self: *Entry, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.tenant_id);
        alloc.free(self.model_id);
        alloc.free(self.secret_ref);
    }
};

pub const CreateParams = struct {
    id: []const u8,
    tenant_id: []const u8,
    model_id: []const u8,
    secret_ref: []const u8,
};

pub fn create(alloc: std.mem.Allocator, conn: *pg.Conn, params: CreateParams) (StateError || anyerror)!Entry {
    const now_ms = clock.nowMillis();
    var q = PgQuery.from(conn.query(sql.INSERT, .{
        params.id,
        params.tenant_id,
        params.model_id,
        params.secret_ref,
        now_ms,
    }) catch |err| {
        if (err == error.PG and isUniqueViolation(conn)) return StateError.DuplicateEntry;
        return err;
    });
    defer q.deinit();

    const row = (try q.next()) orelse return error.RowMissing;
    return rowToEntry(alloc, row);
}

pub fn list(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8) ![]Entry {
    var q = PgQuery.from(try conn.query(sql.LIST, .{tenant_id}));
    defer q.deinit();

    var rows: std.ArrayList(Entry) = .empty;
    errdefer {
        deinitEntriesOnly(rows.items, alloc);
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        var entry = try rowToEntry(alloc, row);
        rows.append(alloc, entry) catch |err| {
            entry.deinit(alloc);
            return err;
        };
    }
    return rows.toOwnedSlice(alloc);
}

/// Where a page starts. `null` is the first page; otherwise the exclusive
/// boundary carried by the caller's cursor.
pub const PageStart = struct {
    created_at: i64,
    id: []const u8,
};

/// One page of entries plus whether another exists. `has_more` is derived from
/// an over-fetch of one row rather than a COUNT, so a page costs one statement
/// regardless of how many entries the tenant has.
pub const Page = struct {
    rows: []Entry,
    has_more: bool,
};

/// Read one page in `created_at DESC, id DESC` order.
///
/// `limit` is the number of rows the caller gets; this asks the database for
/// one more and drops it. That extra row is the whole "is there a next page?"
/// answer — without it the alternatives are a second COUNT statement (which
/// costs a full scan on a keyset read) or returning `next_cursor` unconditionally
/// and making the client discover the end by fetching an empty page.
pub fn listPage(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    limit: u32,
    after: ?PageStart,
) !Page {
    const probe: i32 = @intCast(limit + 1);
    var q = PgQuery.from(if (after) |a|
        try conn.query(sql.LIST_PAGE_AFTER, .{ tenant_id, a.created_at, a.id, probe })
    else
        try conn.query(sql.LIST_PAGE_FIRST, .{ tenant_id, probe }));
    defer q.deinit();

    var rows: std.ArrayList(Entry) = .empty;
    errdefer {
        deinitEntriesOnly(rows.items, alloc);
        rows.deinit(alloc);
    }
    var seen: u32 = 0;
    var has_more = false;
    while (try q.next()) |row| {
        seen += 1;
        // The probe row is read (the cursor must be drained either way) but not
        // materialised — it exists to be counted, not returned.
        if (seen > limit) {
            has_more = true;
            continue;
        }
        var entry = try rowToEntry(alloc, row);
        rows.append(alloc, entry) catch |err| {
            entry.deinit(alloc);
            return err;
        };
    }
    return .{ .rows = try rows.toOwnedSlice(alloc), .has_more = has_more };
}

pub fn updateModel(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    id: []const u8,
    model_id: []const u8,
) (StateError || anyerror)!Entry {
    const now_ms = clock.nowMillis();
    var q = PgQuery.from(conn.query(sql.UPDATE_MODEL, .{ id, tenant_id, model_id, now_ms }) catch |err| {
        if (err == error.PG and isUniqueViolation(conn)) return StateError.DuplicateEntry;
        return err;
    });
    defer q.deinit();

    const row = (try q.next()) orelse return StateError.NotFound;
    return rowToEntry(alloc, row);
}

/// One entry by id, or null when the tenant has no such row. Caller owns the
/// result and must `.deinit(alloc)`.
pub fn getById(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, id: []const u8) !?Entry {
    var q = PgQuery.from(try conn.query(sql.SELECT_BY_ID, .{ id, tenant_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try rowToEntry(alloc, row);
}

pub fn delete(conn: *pg.Conn, tenant_id: []const u8, id: []const u8) !bool {
    const affected = try conn.exec(sql.DELETE, .{ id, tenant_id });
    return (affected orelse 0) > 0;
}

/// Insert the (model_id, secret_ref) registry row for tenant_id if absent —
/// the write-half of the M121 invariant ("every active selection has a
/// matching entry"). A duplicate is a clean no-op (ON CONFLICT DO NOTHING),
/// so repeat activations converge and PUT /provider stays idempotent.
///
/// CALLERS MUST ALREADY HOLD THE CREDENTIAL'S REFERENCE LOCK. This creates a
/// reference to `secret_ref`, so calling it outside a
/// `state/secret_reference_txn.zig` transaction reopens the orphan race that
/// module exists to close: a concurrent credential delete lands between the
/// caller's existence check and this insert, and the entry survives naming a
/// credential that is gone.
///
/// Today the only production caller is `tenant_provider.upsertSelfManaged`,
/// which takes the lock first. This is not enforced by the type system — Zig
/// has no way to demand "an open transaction" as a parameter without threading
/// the handle through purely for its own sake — so it is enforced by there
/// being exactly one caller, and by this comment for the next one.
pub fn ensureEntry(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, model_id: []const u8, secret_ref: []const u8) !void {
    const new_id = try id_format.generateTenantModelEntryId(alloc);
    defer alloc.free(new_id);
    _ = try conn.exec(sql.INSERT_IF_ABSENT, .{ new_id, tenant_id, model_id, secret_ref, clock.nowMillis() });
}

pub fn secretExistsForTenant(conn: *pg.Conn, tenant_id: []const u8, secret_ref: []const u8) !bool {
    // Two statements on purpose: the workspace lookup runs as `api_runtime`,
    // the vault probe under `vault_runtime` (see sql.zig on the split). The
    // primary workspace is stable — created at signup, ordered by creation —
    // so the moment between the statements changes nothing a caller can see.
    var ws_buf: [64]u8 = undefined;
    const ws = blk: {
        var q = PgQuery.from(try conn.query(sql.SELECT_PRIMARY_WORKSPACE, .{tenant_id}));
        defer q.deinit();
        const row = (try q.next()) orelse break :blk null;
        const id = try row.get([]const u8, 0);
        if (id.len == 0 or id.len > ws_buf.len) return error.RowMissing;
        @memcpy(ws_buf[0..id.len], id);
        break :blk ws_buf[0..id.len];
    };
    const workspace_id = ws orelse return false;

    var scope = try pool_elevation.begin(conn, .vault);
    defer scope.deinit();
    const found = blk: {
        var q = PgQuery.from(try scope.query(sql.EXISTS_SECRET_IN_WORKSPACE, .{ workspace_id, secret_ref }));
        defer q.deinit();
        break :blk (try q.next()) != null;
    };
    try scope.commit();
    return found;
}

pub fn referencedSecretCount(conn: *pg.Conn, tenant_id: []const u8, secret_ref: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql.REFERENCED_SECRET_COUNT, .{ tenant_id, secret_ref }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return try row.get(i64, 0);
}

pub fn deinitEntryList(entries: []Entry, alloc: std.mem.Allocator) void {
    deinitEntriesOnly(entries, alloc);
    alloc.free(entries);
}

fn deinitEntriesOnly(entries: []Entry, alloc: std.mem.Allocator) void {
    for (entries) |*entry| entry.deinit(alloc);
}

fn rowToEntry(alloc: std.mem.Allocator, row: anytype) !Entry {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(id);
    const tenant_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(tenant_id);
    const model_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(model_id);
    const secret_ref = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(secret_ref);
    return .{
        .id = id,
        .tenant_id = tenant_id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .created_at = try row.get(i64, 4),
        .updated_at = try row.get(i64, 5),
    };
}

fn isUniqueViolation(conn: *pg.Conn) bool {
    const pg_err = conn.err orelse return false;
    return std.mem.eql(u8, pg_err.code, SQLSTATE_UNIQUE_VIOLATION);
}

test {
    _ = @import("tenant_model_entries_test.zig");
}
