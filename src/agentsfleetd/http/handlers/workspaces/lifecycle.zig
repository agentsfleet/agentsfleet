const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const logging = @import("log");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const provision = @import("provision.zig");
const sql = @import("sql.zig");

const DETAIL_WORKSPACE_NAME_EXISTS = "A workspace with this name already exists";
const DETAIL_WORKSPACE_NAME_INVALID = "Workspace name contains unsupported characters";
const DETAIL_WORKSPACE_NAME_TOO_LONG = "Workspace name must be 128 characters or fewer";
const DETAIL_WORKSPACE_NAME_REQUIRED = "Workspace name is required";
const MAX_WORKSPACE_NAME_CODEPOINTS = 128;
const STATE_NAME_EXISTS = "name_exists";
const C0_CONTROL_MAX = 0x1f;
const C1_CONTROL_MIN = 0x7f;
const C1_CONTROL_MAX = 0x9f;
const ARABIC_LETTER_MARK = 0x061c;
const DIRECTIONAL_MARK_MIN = 0x200e;
const DIRECTIONAL_MARK_MAX = 0x200f;
const UNSAFE_SEPARATOR_MIN = 0x2028;
const UNSAFE_SEPARATOR_MAX = 0x2029;
const BIDI_EMBEDDING_MIN = 0x202a;
const BIDI_EMBEDDING_MAX = 0x202e;
const BIDI_ISOLATE_MIN = 0x2066;
const BIDI_ISOLATE_MAX = 0x2069;
const UNICODE_SPACE_SEPARATOR_MIN = 0x2000;
const UNICODE_SPACE_SEPARATOR_MAX = 0x200a;
const UNICODE_WHITESPACE_CODEPOINTS = [_]u21{
    0x0085,
    0x00a0,
    0x1680,
    0x202f,
    0x205f,
    0x3000,
};

const log = logging.scoped(.http);

const WorkspaceNameMetrics = struct {
    codepoint_count: usize,
    has_content: bool,
};

fn tenantExists(conn: anytype, tenant_id: []const u8) bool {
    var q = PgQuery.from(conn.query(sql.TENANT_EXISTS, .{tenant_id}) catch return true);
    defer q.deinit();
    const row = q.next() catch return true;
    return row != null;
}

fn isUnicodeWhitespace(codepoint: u21) bool {
    if (codepoint >= UNICODE_SPACE_SEPARATOR_MIN and codepoint <= UNICODE_SPACE_SEPARATOR_MAX) {
        return true;
    }
    return std.mem.containsAtLeastScalar(u21, &UNICODE_WHITESPACE_CODEPOINTS, 1, codepoint);
}

fn workspaceNameMetrics(name: []const u8) ?WorkspaceNameMetrics {
    const view = std.unicode.Utf8View.init(name) catch return null;
    var iterator = view.iterator();
    var count: usize = 0;
    var has_content = false;
    while (iterator.nextCodepoint()) |codepoint| {
        if (codepoint <= C0_CONTROL_MAX or
            (codepoint >= C1_CONTROL_MIN and codepoint <= C1_CONTROL_MAX) or
            codepoint == ARABIC_LETTER_MARK or
            (codepoint >= DIRECTIONAL_MARK_MIN and codepoint <= DIRECTIONAL_MARK_MAX) or
            (codepoint >= UNSAFE_SEPARATOR_MIN and codepoint <= UNSAFE_SEPARATOR_MAX) or
            (codepoint >= BIDI_EMBEDDING_MIN and codepoint <= BIDI_EMBEDDING_MAX) or
            (codepoint >= BIDI_ISOLATE_MIN and codepoint <= BIDI_ISOLATE_MAX))
        {
            return null;
        }
        has_content = has_content or !isUnicodeWhitespace(codepoint);
        count += 1;
    }
    return .{ .codepoint_count = count, .has_content = has_content };
}

fn parseCreateInput(hx: hx_mod.Hx, req: *httpz.Request) ?provision.CreateInput {
    const Req = struct {
        name: ?[]const u8 = null,
    };
    const body = req.body() orelse "{}";
    const parsed = std.json.parseFromSliceLeaky(
        Req,
        hx.alloc,
        body,
        .{ .ignore_unknown_fields = true },
    ) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return null;
    };
    const name_raw = parsed.name orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, DETAIL_WORKSPACE_NAME_REQUIRED);
        return null;
    };
    const name_trimmed = std.mem.trim(u8, name_raw, " \t\x0b\x0c\r\n");
    if (name_trimmed.len == 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, DETAIL_WORKSPACE_NAME_REQUIRED);
        return null;
    }
    const metrics = workspaceNameMetrics(name_trimmed) orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, DETAIL_WORKSPACE_NAME_INVALID);
        return null;
    };
    if (!metrics.has_content) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, DETAIL_WORKSPACE_NAME_REQUIRED);
        return null;
    }
    if (metrics.codepoint_count > MAX_WORKSPACE_NAME_CODEPOINTS) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, DETAIL_WORKSPACE_NAME_TOO_LONG);
        return null;
    }
    return .{ .name = name_trimmed };
}

fn writeCreateResponse(hx: hx_mod.Hx, workspace: provision.CreatedWorkspace, tenant_id: []const u8) void {
    hx.ok(.created, .{
        .workspace_id = workspace.workspace_id,
        .name = workspace.name,
        .request_id = workspace.request_id,
        .tenant_id = tenant_id,
    });
}

fn respondToOutcome(hx: hx_mod.Hx, outcome: provision.Outcome, tenant_id: []const u8) void {
    switch (outcome) {
        .created => |workspace| {
            log.debug("workspace_created", .{
                .workspace_id = workspace.workspace_id,
                .tenant_id = tenant_id,
                .name = workspace.name,
            });
            hx.ctx.telemetry.capture(telemetry_mod.WorkspaceCreated, .{
                .distinct_id = hx.principal.user_id orelse "",
                .workspace_id = workspace.workspace_id,
                .tenant_id = tenant_id,
                .request_id = workspace.request_id,
            });
            writeCreateResponse(hx, workspace, tenant_id);
        },
        .name_exists => {
            log.debug("workspace_name_conflict", .{
                .error_code = error_codes.ERR_WORKSPACE_NAME_EXISTS,
                .request_id = hx.req_id,
                .tenant_id = tenant_id,
            });
            common.errorResponseConflict(
                hx.res,
                error_codes.ERR_WORKSPACE_NAME_EXISTS,
                DETAIL_WORKSPACE_NAME_EXISTS,
                hx.req_id,
                STATE_NAME_EXISTS,
            );
        },
        .failed => {},
    }
}

pub fn innerCreateWorkspace(hx: hx_mod.Hx, req: *httpz.Request) void {
    const input = parseCreateInput(hx, req) orelse return;
    const conn = hx.ctx.pool.acquire() catch {
        log.err("workspace_db_acquire_fail", .{
            .error_code = error_codes.ERR_INTERNAL_DB_UNAVAILABLE,
            .op = "create_workspace",
        });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    var tenant_buf: [64]u8 = undefined;
    const tenant_id = common.resolvePrincipalTenant(
        conn,
        hx.principal,
        &tenant_buf,
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Missing tenant context on session");
        return;
    };
    _ = common.setTenantSessionContext(conn, tenant_id);
    if (!tenantExists(conn, tenant_id)) {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Tenant on session does not exist");
        return;
    }
    const outcome = provision.create(conn, hx, tenant_id, input, clock.nowMillis());
    respondToOutcome(hx, outcome, tenant_id);
}
