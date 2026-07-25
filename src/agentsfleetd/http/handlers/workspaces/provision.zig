const std = @import("std");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const id_format = @import("../../../types/id_format.zig");
const heroku_names = @import("../../../state/heroku_names.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const sql = @import("sql.zig");

const S_FAILED_TO_CREATE_WORKSPACE = "Failed to create workspace";
const MAX_NAME_ATTEMPTS: u8 = 8;

pub const CreateInput = struct {
    idempotency_key: ?[]const u8,
    name: ?[]const u8,
};

pub const StoredCreate = struct {
    workspace_id: []const u8,
    name: []const u8,
    request_id: []const u8,
};

pub const Outcome = union(enum) {
    created: StoredCreate,
    replayed: StoredCreate,
    request_mismatch,
    failed,
};

const StoredRow = struct {
    create: StoredCreate,
    request_name: ?[]const u8,
};

const ReplayLookup = union(enum) {
    none,
    replayed: StoredCreate,
    request_mismatch,
    failed,
};

fn sameRequestName(left: ?[]const u8, right: ?[]const u8) bool {
    if (left == null or right == null) return left == null and right == null;
    return std.mem.eql(u8, left.?, right.?);
}

fn findStored(
    conn: anytype,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
    idempotency_key: []const u8,
) !?StoredRow {
    var q = PgQuery.from(try conn.query(sql.FIND_IDEMPOTENT_CREATE, .{
        tenant_id,
        idempotency_key,
    }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    const name = (try row.get(?[]const u8, 1)) orelse return error.InvalidStoredCreate;
    const request_id = (try row.get(?[]const u8, 3)) orelse return error.InvalidStoredCreate;
    return .{
        .create = .{
            .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .name = try alloc.dupe(u8, name),
            .request_id = try alloc.dupe(u8, request_id),
        },
        .request_name = if (try row.get(?[]const u8, 2)) |value|
            try alloc.dupe(u8, value)
        else
            null,
    };
}

fn lookupReplay(
    conn: anytype,
    hx: hx_mod.Hx,
    tenant_id: []const u8,
    idempotency_key: []const u8,
    request_name: ?[]const u8,
) ReplayLookup {
    const stored = findStored(conn, hx.alloc, tenant_id, idempotency_key) catch {
        // mudball-ok: plain user-safe replay lookup failure; no database details
        common.internalOperationError(hx.res, S_FAILED_TO_CREATE_WORKSPACE, hx.req_id);
        return .failed;
    };
    const found = stored orelse return .none;
    if (!sameRequestName(found.request_name, request_name)) return .request_mismatch;
    return .{ .replayed = found.create };
}

fn insertRow(
    conn: anytype,
    workspace_id: []const u8,
    tenant_id: []const u8,
    name: []const u8,
    hx: hx_mod.Hx,
    now_ms: i64,
    input: CreateInput,
) !void {
    _ = try conn.exec(sql.INSERT_WORKSPACE, .{
        workspace_id,
        tenant_id,
        name,
        hx.principal.user_id,
        now_ms,
        input.idempotency_key,
        input.name,
        hx.req_id,
    });
}

fn isUniqueViolation(conn: anytype) bool {
    const pg_err = conn.err orelse return false;
    return std.mem.eql(u8, pg_err.code, "23505");
}

fn created(workspace_id: []const u8, name: []const u8, request_id: []const u8) Outcome {
    return .{ .created = .{
        .workspace_id = workspace_id,
        .name = name,
        .request_id = request_id,
    } };
}

fn afterUniqueViolation(
    conn: anytype,
    hx: hx_mod.Hx,
    tenant_id: []const u8,
    input: CreateInput,
) ReplayLookup {
    const key = input.idempotency_key orelse return .none;
    return lookupReplay(conn, hx, tenant_id, key, input.name);
}

fn insertNamed(
    conn: anytype,
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    tenant_id: []const u8,
    input: CreateInput,
    now_ms: i64,
) Outcome {
    const name = input.name.?;
    insertRow(conn, workspace_id, tenant_id, name, hx, now_ms, input) catch |err| {
        if (err == error.PG and isUniqueViolation(conn)) {
            switch (afterUniqueViolation(conn, hx, tenant_id, input)) {
                .replayed => |stored| return .{ .replayed = stored },
                .request_mismatch => return .request_mismatch,
                .failed => return .failed,
                .none => {},
            }
        }
        common.internalOperationError(hx.res, S_FAILED_TO_CREATE_WORKSPACE, hx.req_id);
        return .failed;
    };
    return created(workspace_id, name, hx.req_id);
}

fn insertGenerated(
    conn: anytype,
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    tenant_id: []const u8,
    input: CreateInput,
    now_ms: i64,
) Outcome {
    var attempt: u8 = 0;
    while (attempt < MAX_NAME_ATTEMPTS) : (attempt += 1) {
        const candidate = heroku_names.generate(hx.alloc) catch {
            common.internalOperationError(hx.res, "Failed to generate workspace name", hx.req_id);
            return .failed;
        };
        if (insertRow(conn, workspace_id, tenant_id, candidate, hx, now_ms, input)) |_| {
            return created(workspace_id, candidate, hx.req_id);
        } else |err| {
            hx.alloc.free(candidate);
            if (err == error.PG and isUniqueViolation(conn)) {
                switch (afterUniqueViolation(conn, hx, tenant_id, input)) {
                    .replayed => |stored| return .{ .replayed = stored },
                    .request_mismatch => return .request_mismatch,
                    .failed => return .failed,
                    .none => continue,
                }
            }
            common.internalOperationError(hx.res, S_FAILED_TO_CREATE_WORKSPACE, hx.req_id);
            return .failed;
        }
    }
    common.internalOperationError(hx.res, "Failed to generate a unique workspace name", hx.req_id);
    return .failed;
}

pub fn create(
    conn: anytype,
    hx: hx_mod.Hx,
    tenant_id: []const u8,
    input: CreateInput,
    now_ms: i64,
) Outcome {
    if (input.idempotency_key) |key| {
        switch (lookupReplay(conn, hx, tenant_id, key, input.name)) {
            .replayed => |stored| return .{ .replayed = stored },
            .request_mismatch => return .request_mismatch,
            .failed => return .failed,
            .none => {},
        }
    }
    const workspace_id = id_format.generateWorkspaceId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to generate workspace id", hx.req_id);
        return .failed;
    };
    if (input.name != null) {
        return insertNamed(conn, hx, workspace_id, tenant_id, input, now_ms);
    }
    return insertGenerated(conn, hx, workspace_id, tenant_id, input, now_ms);
}
