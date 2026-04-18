//! M9_001 §1.0 — POST /v1/execute HTTP handler.
//! Authenticates the caller (zombie session or zmb_ external agent key),
//! resolves zombie_id + workspace_id, then delegates to outbound_proxy.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const error_codes = @import("../../errors/error_registry.zig");
const pipeline = @import("outbound_proxy.zig");
const api_key = @import("../../auth/api_key.zig");

const log = std.log.scoped(.execute);

pub const Context = common.Context;

// ── Request / Response shapes ─────────────────────────────────────────────

const ExecuteInput = struct {
    target: []const u8,
    method: []const u8,
    credential_ref: []const u8,
    body: ?[]const u8 = null,
};

const ExecuteMeta = struct {
    action_id: []const u8,
    firewall_decision: []const u8,
    credential_injected: bool,
    approval_required: bool,
};

const ExecuteResponse = struct {
    status: u16,
    body: []const u8,
    usezombie: ExecuteMeta,
};

// ── Auth result ───────────────────────────────────────────────────────────

const Caller = struct {
    zombie_id: []const u8,     // UUIDv7 string; owned by arena
    workspace_id: []const u8,  // UUIDv7 string; owned by arena
};

// ── Path A: zombie session auth ───────────────────────────────────────────
// Header: Authorization: Session {session_uuid}

fn authFromSession(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    token: []const u8,
) ?Caller {
    var q = PgQuery.from(conn.query(
        \\SELECT s.zombie_id::text, z.workspace_id::text
        \\FROM core.zombie_sessions s
        \\JOIN core.zombies z ON z.id = s.zombie_id
        \\WHERE s.id = $1::uuid
        \\LIMIT 1
    , .{token}) catch return null);
    defer q.deinit();

    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const zombie_id    = row.get([]u8, 0) catch return null;
    const workspace_id = row.get([]u8, 1) catch return null;
    return .{
        .zombie_id    = alloc.dupe(u8, zombie_id)    catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

// ── Path B: zmb_ external agent key auth ─────────────────────────────────
// Header: Authorization: Bearer zmb_xxx
// Constant-time: compare SHA-256 hex of provided key against stored hash (RULE CTM).

fn authFromApiKey(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    raw_key: []const u8,
) ?Caller {
    const hex = api_key.sha256Hex(raw_key);
    const computed_hash: []const u8 = hex[0..];

    var q = PgQuery.from(conn.query(
        \\SELECT ea.key_hash, ea.zombie_id::text, ea.workspace_id::text
        \\FROM core.agent_keys ea
        \\WHERE ea.key_hash = $1
        \\LIMIT 1
    , .{computed_hash}) catch return null);
    defer q.deinit();

    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const stored_hash  = row.get([]u8, 0) catch return null;
    const zombie_id    = row.get([]u8, 1) catch return null;
    const workspace_id = row.get([]u8, 2) catch return null;

    if (!api_key.constantTimeEql(computed_hash, stored_hash)) return null;

    // Best-effort: record last use time. Failure is not fatal.
    _ = conn.exec(
        \\UPDATE core.agent_keys SET last_used_at = $1 WHERE key_hash = $2
    , .{ std.time.milliTimestamp(), computed_hash }) catch {};

    return .{
        .zombie_id    = alloc.dupe(u8, zombie_id)    catch return null,
        .workspace_id = alloc.dupe(u8, workspace_id) catch return null,
    };
}

// ── Auth dispatch ─────────────────────────────────────────────────────────

fn authenticate(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    req: *httpz.Request,
) ?Caller {
    const auth_header = req.header("authorization") orelse return null;

    if (std.mem.startsWith(u8, auth_header, "Session ")) {
        const token = auth_header["Session ".len..];
        return authFromSession(alloc, conn, token);
    }

    const bearer_prefix = "Bearer ";
    if (std.mem.startsWith(u8, auth_header, bearer_prefix)) {
        const token = auth_header[bearer_prefix.len..];
        if (std.mem.startsWith(u8, token, "zmb_")) {
            return authFromApiKey(alloc, conn, token);
        }
    }

    return null;
}

// ── Handler ───────────────────────────────────────────────────────────────

pub fn innerExecute(hx: hx_mod.Hx, req: *httpz.Request) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const caller = authenticate(hx.alloc, conn, req) orelse {
        hx.fail(error_codes.ERR_APIKEY_INVALID,
            "Invalid API key or session. Create one with: zombiectl agent create");
        return;
    };

    const raw_body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(ExecuteInput, hx.alloc, raw_body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;

    if (input.target.len == 0 or input.target.len > 512) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "target must be 1–512 chars");
        return;
    }
    if (input.method.len == 0 or input.method.len > 16) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "method must be 1–16 chars");
        return;
    }

    const result = pipeline.run(hx.alloc, conn, .{
        .zombie_id      = caller.zombie_id,
        .workspace_id   = caller.workspace_id,
        .target         = input.target,
        .method         = input.method,
        .body           = input.body,
        .credential_ref = input.credential_ref,
    }) catch |err| {
        switch (err) {
            error.DomainBlocked      => hx.fail(error_codes.ERR_FW_DOMAIN_BLOCKED,
                "Target domain not mapped to a known service"),
            error.InjectionDetected  => hx.fail(error_codes.ERR_FW_INJECTION_DETECTED,
                "Prompt injection pattern detected in request body"),
            error.ApprovalRequired   => hx.fail(error_codes.ERR_FW_APPROVAL_REQUIRED,
                "Request body requires human approval before execution. Awaiting gate decision."),
            error.GrantNotFound      => hx.fail(error_codes.ERR_GRANT_NOT_FOUND,
                "No approved grant for this service. Request one via POST /v1/zombies/{id}/integration-requests"),
            error.GrantPending       => hx.fail(error_codes.ERR_GRANT_PENDING,
                "Grant pending human approval. Approve in Slack, Discord, or dashboard."),
            error.GrantDenied        => hx.fail(error_codes.ERR_GRANT_DENIED,
                "Grant denied or revoked by workspace owner."),
            error.CredentialNotFound => hx.fail(error_codes.ERR_TOOL_CRED_NOT_FOUND,
                "Credential not found in vault. Add with: zombiectl credential add {ref}"),
            error.TargetError        => hx.fail(error_codes.ERR_PROXY_TARGET_ERROR,
                "Target API unreachable or returned an error."),
            error.OutOfMemory        => common.internalOperationError(hx.res, "Out of memory", hx.req_id),
        }
        return;
    };

    hx.res.header("X-UseZombie-Action-Id", result.action_id);
    hx.res.header("X-UseZombie-Firewall-Decision", result.firewall_decision);
    if (result.truncated) hx.res.header("X-UseZombie-Truncated", "true");

    hx.ok(.ok, ExecuteResponse{
        .status = result.status,
        .body   = result.body,
        .usezombie = .{
            .action_id           = result.action_id,
            .firewall_decision   = result.firewall_decision,
            .credential_injected = result.credential_injected,
            .approval_required   = false,
        },
    });
}
