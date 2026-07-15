//! Per-user, per-workspace dashboard UI preferences.
//!
//! The server stores an opaque small JSON value per named key and never
//! interprets it: the key allowlist (`PrefKey`) and the byte cap
//! (`MAX_PREF_VALUE_BYTES`) are the whole validation surface, so a new
//! client-side toggle costs one enum tag here and one in the TypeScript mirror
//! (`ui/packages/app/lib/api/preferences.ts`) — the tag names ARE the wire strings.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("user_preferences/sql.zig");

/// An opaque pref value is a client-owned blob; without a cap the column is
/// free tenant storage. Mirrored verbatim in the TypeScript client.
pub const MAX_PREF_VALUE_BYTES: usize = 1024;

/// The closed registry of writable pref keys. Tag names are the wire strings —
/// there is no second spelling to drift from (RULE UFS).
pub const PrefKey = enum {
    getting_started_dismissed,
    getting_started_collapsed,
    getting_started_cli_ticked,

    pub fn fromWire(s: []const u8) ?PrefKey {
        return std.meta.stringToEnum(PrefKey, s);
    }

    pub fn wire(self: PrefKey) []const u8 {
        return @tagName(self);
    }
};

pub const Pref = struct {
    key: []const u8,
    /// Raw JSON text exactly as the client wrote it.
    value: []const u8,

    pub fn deinit(self: *Pref, alloc: std.mem.Allocator) void {
        alloc.free(self.key);
        alloc.free(self.value);
    }
};

/// Maps the Clerk subject carried on the principal to the internal user id every
/// prefs row keys on. Null when no user row exists for the subject yet.
/// Caller must free the returned id.
pub fn resolveUserId(alloc: std.mem.Allocator, conn: *pg.Conn, oidc_subject: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(sql.SELECT_USER_ID_BY_SUBJECT, .{oidc_subject}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

/// Every pref this user has set in this workspace. An unset bag is an empty
/// slice, never an error — a caller that cannot read prefs shows onboarding.
pub fn readBag(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    user_id: []const u8,
    workspace_id: []const u8,
) ![]Pref {
    var q = PgQuery.from(try conn.query(sql.SELECT_BAG, .{ user_id, workspace_id }));
    defer q.deinit();

    var rows: std.ArrayList(Pref) = .empty;
    errdefer {
        deinitPrefsOnly(rows.items, alloc);
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        var pref = try rowToPref(alloc, row);
        rows.append(alloc, pref) catch |err| {
            pref.deinit(alloc);
            return err;
        };
    }
    return rows.toOwnedSlice(alloc);
}

/// Writes one key. Last-write-wins by design: a pref is a single scalar toggle,
/// so a lost concurrent write costs one click, not authored content.
pub fn upsert(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    user_id: []const u8,
    workspace_id: []const u8,
    key: PrefKey,
    value: []const u8,
) !void {
    const new_id = try id_format.generateUserPreferenceId(alloc);
    defer alloc.free(new_id);
    _ = try conn.exec(sql.UPSERT_PREF, .{
        new_id,
        user_id,
        workspace_id,
        key.wire(),
        value,
        clock.nowMillis(),
    });
}

pub fn deinitBag(prefs: []Pref, alloc: std.mem.Allocator) void {
    deinitPrefsOnly(prefs, alloc);
    alloc.free(prefs);
}

fn deinitPrefsOnly(prefs: []Pref, alloc: std.mem.Allocator) void {
    for (prefs) |*pref| pref.deinit(alloc);
}

fn rowToPref(alloc: std.mem.Allocator, row: anytype) !Pref {
    const key = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(key);
    const value = try alloc.dupe(u8, try row.get([]const u8, 1));
    return .{ .key = key, .value = value };
}

test {
    _ = @import("user_preferences_test.zig");
}
