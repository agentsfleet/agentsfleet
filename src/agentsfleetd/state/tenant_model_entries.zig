const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
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

pub fn delete(conn: *pg.Conn, tenant_id: []const u8, id: []const u8) !bool {
    const affected = try conn.exec(sql.DELETE, .{ id, tenant_id });
    return (affected orelse 0) > 0;
}

/// Insert the (model_id, secret_ref) registry row for tenant_id if absent —
/// the write-half of the M121 invariant ("every active selection has a
/// matching entry"). A duplicate is a clean no-op (ON CONFLICT DO NOTHING),
/// so repeat activations converge and PUT /provider stays idempotent.
pub fn ensureEntry(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, model_id: []const u8, secret_ref: []const u8) !void {
    const new_id = try id_format.generateTenantModelEntryId(alloc);
    defer alloc.free(new_id);
    _ = try conn.exec(sql.INSERT_IF_ABSENT, .{ new_id, tenant_id, model_id, secret_ref, clock.nowMillis() });
}

pub fn secretExistsForTenant(conn: *pg.Conn, tenant_id: []const u8, secret_ref: []const u8) !bool {
    var q = PgQuery.from(try conn.query(sql.EXISTS_SECRET_IN_PRIMARY_WORKSPACE, .{ tenant_id, secret_ref }));
    defer q.deinit();
    return (try q.next()) != null;
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
