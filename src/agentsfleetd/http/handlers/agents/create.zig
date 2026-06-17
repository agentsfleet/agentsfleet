//! POST /v1/workspaces/{ws}/agents — atomic install. INSERT core.agents →
//! XGROUP CREATE MKSTREAM agent:{id}:events synchronously before the 201, so
//! an event 1ms later finds the consumer group the lease XREADGROUP needs.
//! Post-INSERT group-setup failure rolls the PG row back. A rare double-fault
//! (setup retries exhausted AND rollback also fails) leaves an orphan that is
//! not auto-healed — a control-plane reconcile job is the planned replacement
//! for the deleted worker watcher's sweep (out of scope here).

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const agent_config = @import("../../../agent/config.zig");
const create_stream = @import("create_stream.zig");

const log = logging.scoped(.agent_api);

const Hx = hx_mod.Hx;

/// Max `SKILL.md` / `TRIGGER.md` body sizes — shared with `patch.zig` so an
/// edit cannot smuggle an oversized body past the create-time cap (the body now
/// rides every lease, so the cap is a lease-size guard too).
pub const MAX_SOURCE_LEN: usize = 64 * 1024; // 64KB
pub const MAX_TRIGGER_LEN: usize = 64 * 1024; // 64KB

/// Install request shape. The server is the single parser of TRIGGER.md
/// frontmatter — `name` and `config_json` are derived here, not sent by
/// the CLI. Keeping the contract minimal lets the CLI stay zero-dep
/// (no YAML parser in JS).
const CreateBody = struct {
    trigger_markdown: []const u8,
    source_markdown: []const u8,
};

fn parseCreateBody(hx: Hx, req: *httpz.Request) ?CreateBody {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(CreateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn validateCreateFields(hx: Hx, b: CreateBody) bool {
    if (b.source_markdown.len == 0 or b.source_markdown.len > MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_SOURCE_REQUIRED);
        return false;
    }
    if (b.trigger_markdown.len == 0 or b.trigger_markdown.len > MAX_TRIGGER_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_TRIGGER_REQUIRED);
        return false;
    }
    return true;
}

pub fn innerCreateAgent(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = parseCreateBody(hx, req) orelse return;
    if (!validateCreateFields(hx, body)) return;

    var parsed = agent_config.parseTriggerMarkdownWithJson(hx.alloc, body.trigger_markdown) catch {
        hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_INVALID_CONFIG);
        return;
    };
    defer parsed.deinit(hx.alloc);

    // UZ-APPROVAL-005: reject a malformed gate condition at the write boundary.
    // The runtime parser is lenient (it must read whatever is already stored),
    // so this strict check lives here rather than in parseGatePolicy.
    if (parsed.config.gates) |g| {
        if (agent_config.firstInvalidGateCondition(g.rules) != null) {
            hx.fail(ec.ERR_APPROVAL_CONDITION_INVALID, ec.MSG_APPROVAL_CONDITION_INVALID);
            return;
        }
    }

    // SKILL.md parsing is validate-only: the spec keeps SKILL.md verbatim
    // for the LLM (the SOUL half of the SOUL/CONTRACT split — see the
    // frontmatter schema spec under docs/v*/done/). The parsed metadata
    // (description/version/tags/author/model/when_to_use) exists to enforce
    // required fields + the cross-file `name:` invariant here, then
    // deinit'd below. body.source_markdown is the canonical store; future
    // readers re-parse if they need a field. If a query pattern emerges
    // (e.g. "list agents with model=claude-sonnet-4-6"), promote those
    // fields to columns or a config_json sidecar — don't assume they're
    // already persisted.
    var skill_meta = agent_config.parseSkillMetadata(hx.alloc, body.source_markdown) catch {
        hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_SKILL_INVALID);
        return;
    };
    defer skill_meta.deinit(hx.alloc);

    if (!std.mem.eql(u8, skill_meta.name, parsed.config.name)) {
        hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH);
        return;
    }

    // Placement tags: the SKILL.md frontmatter `tags:` the author already wrote
    // become core.agents.required_tags (matched ⊆ runner.labels at lease time).
    // Empty/absent ⇒ '{}' ⇒ any runner (today's behaviour). The parsed slice is
    // passed straight through as a TEXT[] param — no serialization.
    if (!agent_config.validRequiredTags(skill_meta.tags)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "required tags: max 32 tags, each 1..64 chars");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const agent_id = id_format.generateAgentId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();

    insertAgentOnConn(conn, workspace_id, body, parsed, skill_meta.tags, agent_id, now_ms) catch |err| {
        if (isUniqueViolation(err)) {
            hx.fail(ec.ERR_AGENTSFLEET_NAME_EXISTS, ec.MSG_AGENTSFLEET_NAME_EXISTS);
            return;
        }
        log.err("create_failed", .{ .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    create_stream.ensureEventStream(hx.ctx.queue, agent_id) catch |err| {
        log.err(
            "create_stream_setup_failed",
            .{ .err = @errorName(err), .agent_id = agent_id, .req_id = hx.req_id, .hint = "rolling_back_pg_row" },
        );
        // Roll back the PG row so the caller can retry cleanly without leaving
        // an orphan behind. If the rollback also fails (rare — PG flapping in
        // the same handler), the orphan is not auto-healed: a control-plane
        // reconcile job is the planned replacement for the deleted watcher.
        deleteAgentRow(conn, workspace_id, agent_id) catch |rollback_err| {
            log.err(
                "create_rollback_failed",
                .{ .err = @errorName(rollback_err), .agent_id = agent_id, .req_id = hx.req_id, .hint = "row_orphaned_manual_recovery" },
            );
        };
        common.internalOperationError(hx.res, "event-stream setup failed; install rolled back", hx.req_id);
        return;
    };

    var webhook_urls: std.json.ObjectMap = .empty;
    defer {
        var it = webhook_urls.iterator();
        while (it.next()) |entry| hx.alloc.free(entry.value_ptr.string);
        webhook_urls.deinit(hx.alloc);
    }
    populateWebhookUrls(&webhook_urls, hx.alloc, hx.ctx.api_url, agent_id, parsed.config.triggers) catch {
        common.internalOperationError(hx.res, "webhook_urls generation failed", hx.req_id);
        return;
    };

    log.info("created", .{ .id = agent_id, .name = parsed.config.name, .workspace = workspace_id });
    hx.ok(.created, .{
        .agent_id = agent_id,
        .name = parsed.config.name,
        .status = agent_config.AgentStatus.active.toSlice(),
        .webhook_urls = std.json.Value{ .object = webhook_urls },
    });
}

/// `{ <source>: "<api_url>/v1/webhooks/<agent_id>/<source>" }` per webhook
/// trigger; empty when no webhook variants are declared. Caller owns `map`.
fn populateWebhookUrls(
    map: *std.json.ObjectMap,
    alloc: std.mem.Allocator,
    api_url: []const u8,
    agent_id: []const u8,
    triggers: []const agent_config.AgentTrigger,
) !void {
    for (triggers) |t| switch (t) {
        .webhook => |w| {
            const url = try std.fmt.allocPrint(alloc, "{s}/v1/webhooks/{s}/{s}", .{ api_url, agent_id, w.source });
            errdefer alloc.free(url);
            try map.put(alloc, w.source, .{ .string = url });
        },
        .cron, .api => {},
    };
}

fn insertAgentOnConn(
    conn: *pg.Conn,
    workspace_id: []const u8,
    body: CreateBody,
    parsed: agent_config.ParsedTrigger,
    required_tags: []const []const u8,
    agent_id: []const u8,
    now_ms: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.agents
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, required_tags, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::jsonb, $7, $8::text[], $9, $9)
    , .{
        agent_id,
        workspace_id,
        parsed.config.name,
        body.source_markdown,
        body.trigger_markdown,
        parsed.config_json,
        agent_config.AgentStatus.active.toSlice(),
        required_tags,
        now_ms,
    });
}

/// Roll back a freshly-INSERTed agent row. Workspace-scoped to prevent
/// cross-tenant deletes. Returns errors so the caller can decide whether
/// to log loudly (rare double-fault) or swallow.
fn deleteAgentRow(conn: *pg.Conn, workspace_id: []const u8, agent_id: []const u8) !void {
    _ = try conn.exec(
        \\DELETE FROM core.agents WHERE id = $1::uuid AND workspace_id = $2::uuid
    , .{ agent_id, workspace_id });
}

fn isUniqueViolation(_: anyerror) bool {
    // pg.Pool returns error.PGError for all Postgres errors (connection, constraint, cast).
    // We cannot distinguish unique_violation (SQLSTATE 23505) from other PGErrors
    // because pg.Pool does not expose structured SQLSTATE codes.
    // Return false to let the caller surface a 500 instead of a misleading 409.
    return false;
}

test "isUniqueViolation always returns false (no SQLSTATE introspection)" {
    try std.testing.expect(!isUniqueViolation(error.PGError));
    try std.testing.expect(!isUniqueViolation(error.OutOfMemory));
}
