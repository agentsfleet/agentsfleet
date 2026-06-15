//! `AgentSession` + the claim/checkpoint reads the lease verb's per-event prep
//! needs.
//!
//! Lifted from the worker's `event_loop`/`event_loop_helpers` at the M80
//! cutover. `claimAgent` loads a agent's config + session checkpoint from
//! Postgres and hands back a caller-owned `AgentSession`; the lease verb
//! (`fleet/service.zig`) calls it once per fresh claim. The run-loop that
//! used to wrap it lives only in the deleted worker.
//!
//! Caller-owned allocator: methods that allocate (incl. deinit) take the
//! allocator as a parameter.

const Self = @This();

agent_id: []const u8,
workspace_id: []const u8,
config: agent_config.AgentConfig,
instructions: []const u8,
/// Session context (conversation memory) from core.agent_sessions.
/// JSON string. "{}" for a fresh session.
context_json: []const u8,
/// Source markdown — owns the memory that instructions borrows from.
source_markdown: []const u8,
/// Active execution session handle. NULL when agent is idle.
/// Set at createExecution, cleared at destroyExecution and on claimAgent (crash recovery).
/// Persisted to core.agent_sessions.execution_id so the steer API can read it.
execution_id: ?[]const u8 = null,
/// Millis timestamp when execution_id was set. 0 when idle.
execution_started_at: i64 = 0,

comptime {
    const actual = @sizeOf(Self);
    if (actual != 320) @compileError(std.fmt.comptimePrint("AgentSession size changed: {d}, expected 320", .{actual}));
}

pub fn deinit(self: *Self, alloc: Allocator) void {
    alloc.free(self.agent_id);
    alloc.free(self.workspace_id);
    self.config.deinit(alloc);
    alloc.free(self.source_markdown);
    alloc.free(self.context_json);
    if (self.execution_id) |eid| alloc.free(eid);
}

/// Claim a Agent: load config + session checkpoint from Postgres.
/// Returns a AgentSession that the caller owns and must deinit.
pub fn claimAgent(
    alloc: Allocator,
    agent_id_input: []const u8,
    pool: *pg.Pool,
) !Self {
    // 1. Load agent row from core.agents
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, config_json::text, source_markdown, status
        \\FROM core.agents WHERE id = $1
    , .{agent_id_input}));
    defer q.deinit();

    const row = try q.next() orelse {
        log.warn("agent_event_loop.claim_not_found", .{
            .agent_id = agent_id_input,
            .error_code = error_codes.ERR_AGENTSFLEET_CLAIM_FAILED,
            .reason = "not_found",
        });
        return error.AgentNotFound;
    };

    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const config_json = try alloc.dupe(u8, try row.get([]const u8, 1));
    defer alloc.free(config_json);
    const source_markdown = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(source_markdown);
    // Check status before deinit — row-backed slices are invalid after deinit.
    const status = agent_config.AgentStatus.fromSlice(try row.get([]const u8, 3)) orelse .stopped;

    if (!status.isRunnable()) {
        log.warn("agent_event_loop.claim_skipped", .{ .agent_id = agent_id_input });
        // errdefer on workspace_id and source_markdown fires automatically —
        // no manual free here (would be a double-free).
        return error.AgentNotActive;
    }

    // 2. Parse config
    var config = try agent_config.parseAgentConfig(alloc, config_json);
    errdefer config.deinit(alloc);

    // 3. Extract instructions (borrowed from source_markdown)
    const instructions = agent_config.extractAgentInstructions(source_markdown);

    // 4. Load session checkpoint (or default to fresh)
    const context_json = try loadSessionCheckpoint(alloc, pool, agent_id_input);
    errdefer alloc.free(context_json);

    const agent_id = try alloc.dupe(u8, agent_id_input);
    errdefer alloc.free(agent_id);

    log.info("agent_event_loop.claimed", .{
        .agent_id = agent_id,
        .name = config.name,
        .has_checkpoint = context_json.len > 2,
    });

    var session = Self{
        .agent_id = agent_id,
        .workspace_id = workspace_id,
        .config = config,
        .instructions = instructions,
        .context_json = context_json,
        .source_markdown = source_markdown,
    };
    // Crash recovery: clear any stale execution_id left by a holder that
    // died mid-stage so the next createExecution starts from a clean slot.
    clearExecutionActive(alloc, &session, pool);
    return session;
}

/// Read the persisted session resume cursor (`core.agent_sessions.context_json`),
/// or `"{}"` when the agent has no checkpoint yet.
pub fn loadSessionCheckpoint(alloc: Allocator, pool: *pg.Pool, agent_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT context_json::text FROM core.agent_sessions WHERE agent_id = $1
    , .{agent_id}));
    defer q.deinit();

    if (try q.next()) |row| {
        return alloc.dupe(u8, try row.get([]const u8, 0));
    }
    return alloc.dupe(u8, S_FRESH_CONTEXT);
}

/// Clear active execution in session and DB. Non-fatal — tracking is
/// observability only. Called at claimAgent startup (crash recovery).
pub fn clearExecutionActive(alloc: Allocator, session: *Self, pool: *pg.Pool) void {
    if (session.execution_id) |old| {
        alloc.free(old);
        session.execution_id = null;
    }
    session.execution_started_at = 0;
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.agent_sessions
        \\SET execution_id = NULL, execution_started_at = NULL
        \\WHERE agent_id = $1::uuid
    , .{session.agent_id}) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;
const agent_config = @import("../agent/config.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const log = logging.scoped(.agent_event_loop);
const S_FRESH_CONTEXT = "{}";
