const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const logging = @import("log");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const provision = @import("provision.zig");
const sql = @import("sql.zig");

const S_IDEMPOTENCY_KEY_REUSED = "Idempotency-Key was already used with a different request body";
const S_IDEMPOTENCY_KEY_INVALID = "Idempotency-Key must be a UUIDv7";
const IDEMPOTENCY_KEY_HEADER = "idempotency-key";

const log = logging.scoped(.http);

fn tenantExists(conn: anytype, tenant_id: []const u8) bool {
    var q = PgQuery.from(conn.query(sql.TENANT_EXISTS, .{tenant_id}) catch return true);
    defer q.deinit();
    const row = q.next() catch return true;
    return row != null;
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
    const name_raw = parsed.name orelse "";
    const name_trimmed = std.mem.trim(u8, name_raw, " \t\r\n");
    const name: ?[]const u8 = if (name_trimmed.len == 0) null else name_trimmed;
    const idempotency_key = req.header(IDEMPOTENCY_KEY_HEADER);
    if (idempotency_key) |key| {
        if (!id_format.isUuidV7(key)) {
            hx.fail(error_codes.ERR_INVALID_REQUEST, S_IDEMPOTENCY_KEY_INVALID);
            return null;
        }
    }
    return .{ .idempotency_key = idempotency_key, .name = name };
}

fn writeCreateResponse(hx: hx_mod.Hx, stored: provision.StoredCreate) void {
    hx.ok(.created, .{
        .workspace_id = stored.workspace_id,
        .name = stored.name,
        .request_id = stored.request_id,
    });
}

fn respondToOutcome(hx: hx_mod.Hx, outcome: provision.Outcome, tenant_id: []const u8) void {
    switch (outcome) {
        .created => |stored| {
            log.debug("workspace_created", .{
                .workspace_id = stored.workspace_id,
                .tenant_id = tenant_id,
                .name = stored.name,
            });
            hx.ctx.telemetry.capture(telemetry_mod.WorkspaceCreated, .{
                .distinct_id = hx.principal.user_id orelse "",
                .workspace_id = stored.workspace_id,
                .tenant_id = tenant_id,
                .request_id = stored.request_id,
            });
            writeCreateResponse(hx, stored);
        },
        .replayed => |stored| writeCreateResponse(hx, stored),
        .request_mismatch => {
            hx.fail(error_codes.ERR_INVALID_REQUEST, S_IDEMPOTENCY_KEY_REUSED);
        },
        .failed => {},
    }
}

pub fn innerCreateWorkspace(hx: hx_mod.Hx, req: *httpz.Request) void {
    const input = parseCreateInput(hx, req) orelse return;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Missing tenant context on session");
        return;
    };
    const conn = hx.ctx.pool.acquire() catch {
        log.err("workspace_db_acquire_fail", .{
            .error_code = error_codes.ERR_INTERNAL_DB_UNAVAILABLE,
            .op = "create_workspace",
        });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    _ = common.setTenantSessionContext(conn, tenant_id);
    if (!tenantExists(conn, tenant_id)) {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Tenant on session does not exist");
        return;
    }
    const outcome = provision.create(conn, hx, tenant_id, input, clock.nowMillis());
    respondToOutcome(hx, outcome, tenant_id);
}
