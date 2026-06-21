//! POST /v1/workspaces/{ws}/fleets — atomic install. INSERT core.fleets →
//! XGROUP CREATE MKSTREAM fleet:{id}:events synchronously before the 201, so
//! an event 1ms later finds the consumer group the lease XREADGROUP needs.
//! Post-INSERT group-setup failure rolls the PostgreSQL (PG) row back. A rare double-fault
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
const fleet_config = @import("../../../fleet_runtime/config.zig");
const config_validate = @import("../../../fleet_runtime/config_validate.zig");
const markdown_limits = @import("../../../fleet_runtime/markdown_limits.zig");
const create_stream = @import("create_stream.zig");
const create_fleet_bundle = @import("create_fleet_bundle.zig");

const log = logging.scoped(.fleet_api);

const Hx = hx_mod.Hx;
const DEFAULT_TRIGGER_DAILY_DOLLARS = "1.0";

pub const MAX_SOURCE_LEN = markdown_limits.MAX_SOURCE_LEN;
pub const MAX_TRIGGER_LEN = markdown_limits.MAX_TRIGGER_LEN;

/// Install request shape. The server is the single parser of TRIGGER.md
/// frontmatter — `name` and `config_json` are derived here, not sent by
/// the Command-Line Interface (CLI). `bundle_id` selects a stored Fleet Bundle
/// snapshot while preserving the direct Markdown body path. Keeping the wire
/// shape minimal lets the CLI stay zero-dep (no frontmatter parser in
/// JavaScript (JS)).
const CreateBody = struct {
    bundle_id: ?[]const u8 = null,
    trigger_markdown: ?[]const u8 = null,
    source_markdown: ?[]const u8 = null,
    // Optional operator-supplied name. Absent ⇒ the SKILL.md `name:` is used.
    // Present ⇒ overrides the persisted fleet name so one bundle can back many
    // fleets in a workspace (each with its own name + webhooks/cron).
    name: ?[]const u8 = null,
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

pub fn innerCreateFleet(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = parseCreateBody(hx, req) orelse return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const source = create_fleet_bundle.resolveSource(hx, conn, workspace_id, .{
        .source_markdown = body.source_markdown,
        .trigger_markdown = body.trigger_markdown,
        .bundle_id = body.bundle_id,
    }) orelse return;
    defer source.deinit(hx.alloc);
    if (!create_fleet_bundle.validateFields(hx, source.source_markdown, source.trigger_markdown)) return;

    // SKILL.md parsing is validate-only: the spec keeps SKILL.md verbatim
    // for the Large Language Model (LLM) (the SOUL half of the SOUL/runtime
    // split — see the frontmatter schema spec under docs/v*/done/). The parsed metadata
    // (description/version/tags/author/model/when_to_use) exists to enforce
    // required fields + the cross-file `name:` invariant here, then
    // deinit'd below. body.source_markdown is the canonical store; future
    // readers re-parse if they need a field. If a query pattern emerges
    // (e.g. "list fleets with model=claude-sonnet-4-6"), promote those
    // fields to columns or a config_json sidecar — don't assume they're
    // already persisted.
    var skill_meta = fleet_config.parseSkillMetadata(hx.alloc, source.source_markdown) catch {
        hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_SKILL_INVALID);
        return;
    };
    defer skill_meta.deinit(hx.alloc);

    const trigger_markdown = if (source.trigger_markdown) |tm| tm else buildDefaultTriggerMarkdown(hx.alloc, skill_meta.name) catch {
        common.internalOperationError(hx.res, "trigger generation failed", hx.req_id);
        return;
    };
    defer if (source.trigger_markdown == null) hx.alloc.free(trigger_markdown);

    var parsed = fleet_config.parseTriggerMarkdownWithJson(hx.alloc, trigger_markdown) catch {
        hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_INVALID_CONFIG);
        return;
    };
    defer parsed.deinit(hx.alloc);

    // UZ-APPROVAL-005: reject a malformed gate condition at the write boundary.
    // The runtime parser is lenient (it must read whatever is already stored),
    // so this strict check lives here rather than in parseGatePolicy.
    if (parsed.config.gates) |g| {
        if (fleet_config.firstInvalidGateCondition(g.rules) != null) {
            hx.fail(ec.ERR_APPROVAL_CONDITION_INVALID, ec.MSG_APPROVAL_CONDITION_INVALID);
            return;
        }
    }

    if (!std.mem.eql(u8, skill_meta.name, parsed.config.name)) {
        hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH);
        return;
    }

    // Optional operator name override (multi-instance): the same bundle can back
    // many fleets in a workspace, each with its own name + webhooks/cron. The
    // runner leases by content_hash (name-agnostic) and nothing reads
    // config_json's name downstream, so overriding the persisted `name` column
    // is safe. Validated against the same slug rules as a SKILL.md name.
    if (body.name) |override_name| {
        config_validate.validateSkillName(override_name) catch {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_NAME_REQUIRED);
            return;
        };
        parsed.config.name = override_name;
    }

    // Placement tags: the SKILL.md frontmatter `tags:` the author already wrote
    // become core.fleets.required_tags (matched ⊆ runner.labels at lease time).
    // Empty/absent ⇒ '{}' ⇒ any runner (today's behaviour). The parsed slice is
    // passed straight through as a TEXT[] param — no serialization.
    if (!fleet_config.validRequiredTags(skill_meta.tags)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "required tags: max 32 tags, each 1..64 chars");
        return;
    }

    if (!create_fleet_bundle.ensureBundleCredentials(hx, conn, workspace_id, source.bundle_ref, parsed.config.credentials)) return;

    const fleet_id = id_format.generateFleetId(hx.alloc) catch {
        common.internalOperationError(hx.res, "identifier generation failed", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();

    insertFleetOnConn(conn, workspace_id, source.source_markdown, trigger_markdown, parsed, skill_meta.tags, source.bundle_ref, fleet_id, now_ms) catch |err| {
        if (isUniqueViolation(conn)) {
            hx.fail(ec.ERR_AGENTSFLEET_NAME_EXISTS, ec.MSG_AGENTSFLEET_NAME_EXISTS);
            return;
        }
        log.err("create_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    create_stream.ensureEventStream(hx.ctx.queue, fleet_id) catch |err| {
        log.err(
            "create_stream_setup_failed",
            .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .fleet_id = fleet_id, .req_id = hx.req_id, .hint = "rolling_back_pg_row" },
        );
        // Roll back the PG row so the caller can retry cleanly without leaving
        // an orphan behind. If the rollback also fails (rare — PG flapping in
        // the same handler), the orphan is not auto-healed: a control-plane
        // reconcile job is the planned replacement for the deleted watcher.
        deleteFleetRow(conn, workspace_id, fleet_id) catch |rollback_err| {
            log.err(
                "create_rollback_failed",
                .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(rollback_err), .fleet_id = fleet_id, .req_id = hx.req_id, .hint = "row_orphaned_manual_recovery" },
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
    populateWebhookUrls(&webhook_urls, hx.alloc, hx.ctx.api_url, fleet_id, parsed.config.triggers) catch {
        common.internalOperationError(hx.res, "webhook_urls generation failed", hx.req_id);
        return;
    };

    log.debug("created", .{ .id = fleet_id, .name = parsed.config.name, .workspace = workspace_id });
    hx.ok(.created, .{
        .fleet_id = fleet_id,
        .name = parsed.config.name,
        .status = fleet_config.FleetStatus.active.toSlice(),
        .webhook_urls = std.json.Value{ .object = webhook_urls },
    });
}

/// `{ <source>: "<api_url>/v1/webhooks/<fleet_id>/<source>" }` per webhook
/// trigger; empty when no webhook variants are declared. Caller owns `map`.
fn populateWebhookUrls(
    map: *std.json.ObjectMap,
    alloc: std.mem.Allocator,
    api_url: []const u8,
    fleet_id: []const u8,
    triggers: []const fleet_config.FleetTrigger,
) !void {
    for (triggers) |t| switch (t) {
        .webhook => |w| {
            const url = try std.fmt.allocPrint(alloc, "{s}/v1/webhooks/{s}/{s}", .{ api_url, fleet_id, w.source });
            errdefer alloc.free(url);
            try map.put(alloc, w.source, .{ .string = url });
        },
        .cron, .api => {},
    };
}

fn buildDefaultTriggerMarkdown(alloc: std.mem.Allocator, name: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc,
        \\---
        \\name: {s}
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: api
        \\  tools: []
        \\  budget:
        \\    daily_dollars: {s}
        \\---
        \\
    , .{ name, DEFAULT_TRIGGER_DAILY_DOLLARS });
}

fn insertFleetOnConn(
    conn: *pg.Conn,
    workspace_id: []const u8,
    source_markdown: []const u8,
    trigger_markdown: []const u8,
    parsed: fleet_config.ParsedTrigger,
    required_tags: []const []const u8,
    bundle_ref: ?create_fleet_bundle.BundleRef,
    fleet_id: []const u8,
    now_ms: i64,
) !void {
    const bundle_id: ?[]const u8 = if (bundle_ref) |b| b.id else null;
    const bundle_hash: ?[]const u8 = if (bundle_ref) |b| b.content_hash else null;
    const bundle_key: ?[]const u8 = if (bundle_ref) |b| b.snapshot_key else null;
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, required_tags, bundle_id, bundle_content_hash,
        \\   bundle_snapshot_key, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::jsonb, $7, $8::text[],
        \\        $9::uuid, $10, $11, $12, $12)
    , .{
        fleet_id,
        workspace_id,
        parsed.config.name,
        source_markdown,
        trigger_markdown,
        parsed.config_json,
        fleet_config.FleetStatus.active.toSlice(),
        required_tags,
        bundle_id,
        bundle_hash,
        bundle_key,
        now_ms,
    });
}

/// Roll back a freshly-INSERTed fleet row. Workspace-scoped to prevent
/// cross-tenant deletes. Returns errors so the caller can decide whether
/// to log loudly (rare double-fault) or swallow.
fn deleteFleetRow(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8) !void {
    _ = try conn.exec(
        \\DELETE FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid
    , .{ fleet_id, workspace_id });
}

/// True when the last statement on `conn` failed the `uq_fleets_workspace_id_name`
/// unique constraint (a duplicate fleet name in the workspace). The pg driver
/// surfaces the structured SQLSTATE on `conn.err` after a failed `exec`, so the
/// 409 path is reachable — same introspection the api-keys and signup handlers use.
fn isUniqueViolation(conn: *pg.Conn) bool {
    const pg_err = conn.err orelse return false;
    return isUniqueViolationCode(pg_err.code);
}

/// SQLSTATE `23505` is `unique_violation`.
fn isUniqueViolationCode(sqlstate: []const u8) bool {
    return std.mem.eql(u8, sqlstate, "23505");
}

test "isUniqueViolationCode matches 23505 only" {
    try std.testing.expect(isUniqueViolationCode("23505"));
    try std.testing.expect(!isUniqueViolationCode("23503")); // foreign_key_violation
    try std.testing.expect(!isUniqueViolationCode("XX000"));
    try std.testing.expect(!isUniqueViolationCode(""));
}

test "buildDefaultTriggerMarkdown creates an API trigger" {
    const alloc = std.testing.allocator;
    const trigger = try buildDefaultTriggerMarkdown(alloc, "skill-only-install-pin");
    defer alloc.free(trigger);
    try std.testing.expect(std.mem.indexOf(u8, trigger, "name: skill-only-install-pin") != null);
    try std.testing.expect(std.mem.indexOf(u8, trigger, "type: api") != null);
    try std.testing.expect(std.mem.indexOf(u8, trigger, "tools: []") != null);
    try std.testing.expect(std.mem.indexOf(u8, trigger, "daily_dollars: 1.0") != null);
}
