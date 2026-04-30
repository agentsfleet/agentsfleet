// Zombie event loop helpers — extracted per RULE FLL (350-line gate).
//
// Session checkpoint, credential resolution, context update, truncation, backoff,
// execution tracking, sandbox execution. Called by event_loop.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const error_codes = @import("../errors/error_registry.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const backoff = @import("../reliability/backoff.zig");
const metrics_counters = @import("../observability/metrics_counters.zig");
const metrics_workspace = @import("../observability/metrics_workspace.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const executor_client = @import("../executor/client.zig");
const executor_transport = @import("../executor/transport.zig");
const context_budget = @import("../executor/context_budget.zig");
const id_format = @import("../types/id_format.zig");

const types = @import("event_loop_types.zig");
const event_loop_secrets = @import("event_loop_secrets.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;

const log = std.log.scoped(.zombie_event_loop);

/// Iterate session.config.credentials and return the first successfully
/// resolved value from vault.secrets. Credentials are plain names (e.g. "agentmail").
pub fn resolveFirstCredential(alloc: Allocator, pool: *pg.Pool, session: *ZombieSession) ![]const u8 {
    for (session.config.credentials) |cred_name| {
        return resolveCredential(alloc, pool, session.workspace_id, cred_name) catch continue;
    }
    return error.CredentialNotFound;
}

/// M2_001: Resolve a zombie credential from vault.secrets via crypto_store.
/// Key naming: "zombie:{name}" in vault.secrets. Returns decrypted value.
fn resolveCredential(alloc: Allocator, pool: *pg.Pool, workspace_id: []const u8, name: []const u8) ![]const u8 {
    const key_name = try std.fmt.allocPrint(alloc, "zombie:{s}", .{name});
    defer alloc.free(key_name);

    const conn = try pool.acquire();
    defer pool.release(conn);
    return crypto_store.load(alloc, conn, workspace_id, key_name) catch |err| {
        log.warn("zombie_event_loop.credential_not_found workspace_id={s} name={s} error_code=" ++ error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING, .{ workspace_id, name });
        return err;
    };
}

pub fn loadSessionCheckpoint(alloc: Allocator, pool: *pg.Pool, zombie_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT context_json::text FROM core.zombie_sessions WHERE zombie_id = $1
    , .{zombie_id}));
    defer q.deinit();

    if (try q.next()) |row| {
        return try alloc.dupe(u8, try row.get([]const u8, 0));
    }
    return try alloc.dupe(u8, "{}");
}

pub fn updateSessionContext(
    alloc: Allocator,
    session: *ZombieSession,
    event_id: []const u8,
    agent_response: []const u8,
) !void {
    const truncated = truncateForJson(agent_response);

    const ContextUpdate = struct {
        last_event_id: []const u8,
        last_response: []const u8,
    };

    const new_context = try std.json.Stringify.valueAlloc(alloc, ContextUpdate{
        .last_event_id = event_id,
        .last_response = truncated,
    }, .{});

    alloc.free(session.context_json);
    session.context_json = new_context;
}

/// Truncate a string for safe inclusion in a JSON value (max 2048 bytes).
/// Walks backward from the cut point to avoid splitting a multi-byte UTF-8 sequence.
pub fn truncateForJson(s: []const u8) []const u8 {
    const max_len: usize = 2048;
    if (s.len <= max_len) return s;
    var end = max_len;
    while (end > 0 and (s[end] & 0xC0) == 0x80) end -= 1;
    return s[0..end];
}

pub fn sleepWithBackoff(cfg: EventLoopConfig, consecutive_errors: u32) void {
    const max_delay_ms = std.math.mul(u64, cfg.poll_interval_ms, 8) catch cfg.poll_interval_ms;
    const delay_ms = backoff.expBackoffJitter(
        if (consecutive_errors > 0) consecutive_errors - 1 else 0,
        cfg.poll_interval_ms,
        max_delay_ms,
    );
    var remaining = delay_ms;
    while (remaining > 0 and cfg.running.load(.acquire)) {
        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

// M15_002: exit_status label values for PostHog ZombieCompleted events.
const EXIT_PROCESSED = "processed";
const EXIT_AGENT_ERROR = "agent_error";
const EXIT_DELIVER_ERROR = "deliver_error";

/// M15_002: delivery bookkeeping — activity log + Prometheus + PostHog.
/// Called from event_loop.processEvent after deliverEvent resolves.
pub fn logDeliveryResult(
    cfg: EventLoopConfig,
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    stage_result: anytype,
    wall_ms: u64,
) void {
    _ = alloc;
    const ok = stage_result.failure == null;
    if (ok) {
        log.info("zombie_event_loop.delivered zombie_id={s} event_id={s} tokens={d} wall_s={d}", .{
            session.zombie_id, event.event_id, stage_result.token_count, stage_result.wall_seconds,
        });
        metrics_counters.incZombiesCompleted();
        metrics_counters.addZombieTokens(stage_result.token_count);
        metrics_workspace.addTokens(session.workspace_id, session.zombie_id, stage_result.token_count);
        metrics_counters.observeZombieExecutionSeconds(wall_ms);
    } else {
        const label = stage_result.failure.?.label();
        log.warn("zombie_event_loop.agent_failure zombie_id={s} event_id={s} failure={s}", .{
            session.zombie_id, event.event_id, label,
        });
        metrics_counters.incZombiesFailed();
    }
    // Worker thread has no user context — distinct_id = workspace_id so events
    // group under the owning workspace (consistent with ZombieTriggered in webhooks).
    if (cfg.telemetry) |tel| {
        tel.capture(telemetry_mod.ZombieCompleted, .{
            .distinct_id = session.workspace_id,
            .workspace_id = session.workspace_id,
            .zombie_id = session.zombie_id,
            .event_id = event.event_id,
            .tokens = stage_result.token_count,
            .wall_ms = wall_ms,
            .exit_status = if (ok) EXIT_PROCESSED else EXIT_AGENT_ERROR,
            .time_to_first_token_ms = stage_result.time_to_first_token_ms,
        });
    }
}

/// M15_002: deliver-path failure (before stage_result is available).
pub fn recordDeliverError(cfg: EventLoopConfig, session: *ZombieSession, event_id: []const u8) void {
    metrics_counters.incZombiesFailed();
    if (cfg.telemetry) |tel| {
        tel.capture(telemetry_mod.ZombieCompleted, .{
            .distinct_id = session.workspace_id,
            .workspace_id = session.workspace_id,
            .zombie_id = session.zombie_id,
            .event_id = event_id,
            .tokens = 0,
            .wall_ms = 0,
            .exit_status = EXIT_DELIVER_ERROR,
        });
    }
}

// ── M23_001: Execution tracking ──────────────────────────────────────────────

/// Set active execution in session and DB. Non-fatal — tracking is observability only.
/// Called immediately after createExecution succeeds.
pub fn setExecutionActive(alloc: Allocator, session: *ZombieSession, execution_id: []const u8, pool: *pg.Pool) void {
    const owned = alloc.dupe(u8, execution_id) catch return;
    if (session.execution_id) |old| alloc.free(old);
    session.execution_id = owned;
    session.execution_started_at = std.time.milliTimestamp();
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.zombie_sessions
        \\SET execution_id = $1, execution_started_at = $2
        \\WHERE zombie_id = $3::uuid
    , .{ owned, session.execution_started_at, session.zombie_id }) catch {};
}

/// Clear active execution in session and DB. Non-fatal.
/// Called in defer after destroyExecution, and at claimZombie startup (crash recovery).
pub fn clearExecutionActive(alloc: Allocator, session: *ZombieSession, pool: *pg.Pool) void {
    if (session.execution_id) |old| {
        alloc.free(old);
        session.execution_id = null;
    }
    session.execution_started_at = 0;
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.zombie_sessions
        \\SET execution_id = NULL, execution_started_at = NULL
        \\WHERE zombie_id = $1::uuid
    , .{session.zombie_id}) catch {};
}

// ── Sandbox execution (moved from event_loop.zig for RULE FLL) ───────────────

/// Parse the session's stored context JSON. Returns the full
/// `std.json.Parsed` so the caller can `deinit()` it and reclaim the
/// arena — the previous shape returned `parsed.value` and dropped the
/// arena handle, leaking the parse buffer on every event after the
/// first (because `updateSessionContext` rewrites context_json to a
/// non-empty value the second event then has to parse).
fn parseSessionContext(alloc: Allocator, json: []const u8) ?std.json.Parsed(std.json.Value) {
    if (json.len <= 2) return null;
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch null;
}

/// Create executor session, run the stage, destroy session. Tracks execution_id
/// in session + DB for API visibility. The emitter dispatches each progress
/// frame the executor streams back — pass `ProgressEmitter.noop()` to opt out.
pub fn executeInSandbox(
    alloc: Allocator,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    cfg: EventLoopConfig,
    emitter: executor_transport.ProgressEmitter,
) !executor_client.ExecutorClient.StageResult {
    var context_parsed = parseSessionContext(alloc, session.context_json);
    defer if (context_parsed) |*p| p.deinit();
    const context_val: ?std.json.Value = if (context_parsed) |p| p.value else null;

    // trace_id and session_id both bind to event.event_id: no upstream trace
    // propagator exists yet, so the per-event ID doubles as both the distributed
    // trace handle and the per-turn session identifier.
    // Build the secrets_map from the zombie's configured credential names.
    // The slice is owned here; createExecution serialises it across the RPC
    // boundary (handler deep-dupes into the session arena), so the resolved
    // slice can be freed as soon as the call returns. An empty credential
    // list or a vault failure produces a null secrets_map — the agent will
    // surface `secret_not_found` if it tries to substitute against it.
    var secrets_obj_alive = false;
    var secrets_obj = std.json.ObjectMap.init(alloc);
    defer if (secrets_obj_alive) secrets_obj.deinit();
    var resolved_secrets: ?[]event_loop_secrets.ResolvedSecret = null;
    defer if (resolved_secrets) |r| event_loop_secrets.freeResolved(alloc, r);

    if (session.config.credentials.len > 0) {
        if (event_loop_secrets.resolveSecretsMap(alloc, cfg.pool, session.workspace_id, session.config.credentials)) |r| {
            resolved_secrets = r;
            for (r) |entry| {
                secrets_obj.put(entry.name, entry.parsed.value) catch {};
            }
            secrets_obj_alive = true;
        } else |err| {
            log.warn("zombie_event_loop.secrets_resolve_failed zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
        }
    }
    const secrets_map: ?std.json.Value = if (secrets_obj_alive) .{ .object = secrets_obj } else null;

    // §8 auto-defaults: every empty/zero knob is the auto sentinel.
    // applyContextDefaults substitutes spec defaults so the executor
    // receives a fully-populated ContextBudget. Frontmatter overrides
    // (x-usezombie.context) land here once the parser ships — the
    // parser writes non-zero values, applyContextDefaults leaves them
    // alone.
    var ctx_budget: context_budget.ContextBudget = .{};
    context_budget.applyContextDefaults(&ctx_budget);
    log.info("zombie_event_loop.context_budget_resolved zombie_id={s} tool_window={d} memory_checkpoint_every={d} stage_chunk_threshold={d:.2} context_cap_tokens={d}", .{
        session.zombie_id,
        ctx_budget.tool_window,
        ctx_budget.memory_checkpoint_every,
        ctx_budget.stage_chunk_threshold,
        ctx_budget.context_cap_tokens,
    });

    const execution_id = cfg.executor.createExecution(.{
        .workspace_path = cfg.workspace_path,
        .correlation = .{
            .trace_id = event.event_id,
            .zombie_id = session.zombie_id,
            .workspace_id = session.workspace_id,
            .session_id = event.event_id,
        },
        .secrets_map = secrets_map,
        .context = ctx_budget,
        // network_policy / tools remain default empty — slice 3 wires
        // per-zombie network allowlist + tool filter from frontmatter.
    }) catch |err| {
        log.err("zombie_event_loop.exec_create_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_SESSION_CREATE_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
    // clearExecutionActive defer runs BEFORE this (LIFO), so DB is cleared before socket is closed.
    defer {
        cfg.executor.destroyExecution(execution_id) catch {};
        alloc.free(execution_id);
    }

    // M23_001: track execution in session + DB so the steer API can read it.
    setExecutionActive(alloc, session, execution_id, cfg.pool);
    defer clearExecutionActive(alloc, session, cfg.pool);

    const api_key: []const u8 = resolveFirstCredential(alloc, cfg.pool, session) catch "";
    defer if (api_key.len > 0) alloc.free(api_key);

    // Extract the agent-facing message from the envelope's structured
    // request payload (`{"message": "...", "metadata": {...}}`). Fall back
    // to the raw request JSON when the field is missing, so producers that
    // forget to wrap still deliver something.
    var owned_message: ?[]u8 = null;
    defer if (owned_message) |m| alloc.free(m);
    const message_text: []const u8 = blk: {
        var parsed = std.json.parseFromSlice(std.json.Value, alloc, event.request_json, .{}) catch break :blk event.request_json;
        defer parsed.deinit();
        if (parsed.value != .object) break :blk event.request_json;
        const msg_val = parsed.value.object.get("message") orelse break :blk event.request_json;
        if (msg_val != .string) break :blk event.request_json;
        const dup = alloc.dupe(u8, msg_val.string) catch break :blk event.request_json;
        owned_message = dup;
        break :blk dup;
    };

    return cfg.executor.startStageStreaming(execution_id, .{
        .agent_config = .{
            .system_prompt = session.instructions,
            .api_key = api_key,
        },
        .message = message_text,
        .context = context_val,
    }, emitter) catch |err| {
        log.err("zombie_event_loop.stage_fail zombie_id={s} event_id={s} error_code=" ++ error_codes.ERR_EXEC_STAGE_START_FAILED, .{ session.zombie_id, event.event_id });
        return err;
    };
}
