const std = @import("std");
const id_format = @import("../../../types/id_format.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const sql = @import("sql.zig");

const S_FAILED_TO_CREATE_WORKSPACE = "Failed to create workspace";
const POSTGRES_UNIQUE_VIOLATION = "23505";
const UNIQUE_WORKSPACE_NAME_CONSTRAINT = "uq_workspaces_tenant_name";

pub const CreateInput = struct {
    name: []const u8,
};

pub const CreatedWorkspace = struct {
    workspace_id: []const u8,
    name: []const u8,
    request_id: []const u8,
};

pub const Outcome = union(enum) {
    created: CreatedWorkspace,
    name_exists,
    failed,
};

fn insertRow(
    conn: anytype,
    workspace_id: []const u8,
    tenant_id: []const u8,
    name: []const u8,
    hx: hx_mod.Hx,
    now_ms: i64,
) !void {
    _ = try conn.exec(sql.INSERT_WORKSPACE, .{
        workspace_id,
        tenant_id,
        name,
        hx.principal.user_id,
        now_ms,
    });
}

fn isWorkspaceNameConstraint(code: []const u8, constraint: ?[]const u8) bool {
    const constraint_name = constraint orelse return false;
    return std.mem.eql(u8, code, POSTGRES_UNIQUE_VIOLATION) and
        std.mem.eql(u8, constraint_name, UNIQUE_WORKSPACE_NAME_CONSTRAINT);
}

fn isWorkspaceNameConflict(conn: anytype) bool {
    const pg_err = conn.err orelse return false;
    return isWorkspaceNameConstraint(pg_err.code, pg_err.constraint);
}

test "unit: workspace name conflict classification is constraint exact" {
    try std.testing.expect(isWorkspaceNameConstraint(
        POSTGRES_UNIQUE_VIOLATION,
        UNIQUE_WORKSPACE_NAME_CONSTRAINT,
    ));
    try std.testing.expect(!isWorkspaceNameConstraint(
        POSTGRES_UNIQUE_VIOLATION,
        "uq_workspaces_other",
    ));
    try std.testing.expect(!isWorkspaceNameConstraint("23514", UNIQUE_WORKSPACE_NAME_CONSTRAINT));
    try std.testing.expect(!isWorkspaceNameConstraint(POSTGRES_UNIQUE_VIOLATION, null));
}

fn created(workspace_id: []const u8, name: []const u8, request_id: []const u8) Outcome {
    return .{ .created = .{
        .workspace_id = workspace_id,
        .name = name,
        .request_id = request_id,
    } };
}

fn insert(
    conn: anytype,
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    tenant_id: []const u8,
    name: []const u8,
    now_ms: i64,
) Outcome {
    insertRow(conn, workspace_id, tenant_id, name, hx, now_ms) catch |err| {
        if (err == error.PG and isWorkspaceNameConflict(conn)) return .name_exists;
        common.internalOperationError(hx.res, S_FAILED_TO_CREATE_WORKSPACE, hx.req_id);
        return .failed;
    };
    return created(workspace_id, name, hx.req_id);
}

pub fn create(
    conn: anytype,
    hx: hx_mod.Hx,
    tenant_id: []const u8,
    input: CreateInput,
    now_ms: i64,
) Outcome {
    const workspace_id = id_format.generateWorkspaceId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to generate workspace id", hx.req_id);
        return .failed;
    };
    return insert(conn, hx, workspace_id, tenant_id, input.name, now_ms);
}
