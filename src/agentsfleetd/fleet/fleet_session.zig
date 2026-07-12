//! `FleetSession` + the claim/checkpoint reads the lease verb's per-event prep
//! needs.
//!
//! Lifted from the worker's `event_loop`/`event_loop_helpers` at the M80
//! cutover. `claimFleet` loads a fleet's config + session checkpoint from
//! Postgres and hands back a caller-owned `FleetSession`; the lease verb
//! (`fleet/service.zig`) calls it once per fresh claim. The run-loop that
//! used to wrap it lives only in the deleted worker.
//!
//! Caller-owned allocator: methods that allocate (incl. deinit) take the
//! allocator as a parameter.

const Self = @This();

fleet_id: []const u8,
workspace_id: []const u8,
config: fleet_config.FleetConfig,
instructions: []const u8,
/// Session context (conversation memory) from core.fleet_sessions.
/// JSON string. "{}" for a fresh session.
context_json: []const u8,
/// Source markdown — owns the memory that instructions borrows from.
source_markdown: []const u8,
/// Content hash of the installed Fleet Bundle's snapshot, or null when the fleet
/// was not created from a bundle. Flows onto the lease so the runner downloads +
/// materializes the canonical tar (never the raw upstream archive).
bundle_content_hash: ?[]const u8 = null,
/// Active execution session handle. NULL when fleet is idle.
/// Set at createExecution, cleared at destroyExecution and on claimFleet (crash recovery).
/// Persisted to core.fleet_sessions.execution_id so the steer API can read it.
execution_id: ?[]const u8 = null,
/// Millis timestamp when execution_id was set. 0 when idle.
execution_started_at: i64 = 0,

comptime {
    const actual = @sizeOf(Self);
    if (actual != 336) @compileError(std.fmt.comptimePrint("FleetSession size changed: {d}, expected 336", .{actual}));
}

pub fn deinit(self: *Self, alloc: Allocator) void {
    alloc.free(self.fleet_id);
    alloc.free(self.workspace_id);
    self.config.deinit(alloc);
    alloc.free(self.source_markdown);
    alloc.free(self.context_json);
    if (self.bundle_content_hash) |bch| alloc.free(bch);
    if (self.execution_id) |eid| alloc.free(eid);
    self.* = undefined;
}

/// Claim a Fleet: load config + session checkpoint from Postgres.
/// Returns a FleetSession that the caller owns and must deinit.
pub fn claimFleet(
    alloc: Allocator,
    fleet_id_input: []const u8,
    pool: *pg.Pool,
) !Self {
    // 1. Load fleet row from core.fleets
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, config_json::text, source_markdown, status, bundle_content_hash
        \\FROM core.fleets WHERE id = $1
    , .{fleet_id_input}));
    defer q.deinit();

    const row = try q.next() orelse {
        log.warn("fleet_event_loop_claim_not_found", .{
            .fleet_id = fleet_id_input,
            .error_code = error_codes.ERR_AGENTSFLEET_CLAIM_FAILED,
            .reason = "not_found",
        });
        return error.FleetNotFound;
    };

    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const config_json = try alloc.dupe(u8, try row.get([]const u8, 1));
    defer alloc.free(config_json);
    const source_markdown = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(source_markdown);
    // Check status before deinit — row-backed slices are invalid after deinit.
    const status = fleet_config.FleetStatus.fromSlice(try row.get([]const u8, 3)) orelse .stopped;
    // Bundle ref (nullable column): present only for fleets created from a bundle.
    const bundle_content_hash: ?[]const u8 = if (try row.get(?[]const u8, 4)) |bch| try alloc.dupe(u8, bch) else null;
    errdefer if (bundle_content_hash) |bch| alloc.free(bch);

    if (!status.isRunnable()) {
        log.warn("fleet_event_loop_claim_skipped", .{ .error_code = error_codes.ERR_AGENTSFLEET_PAUSED_INGRESS, .fleet_id = fleet_id_input });
        // errdefer on workspace_id and source_markdown fires automatically —
        // no manual free here (would be a double-free).
        return error.FleetNotActive;
    }

    // 2. Parse config
    var config = try fleet_config.parseFleetConfig(alloc, config_json);
    errdefer config.deinit(alloc);

    // 3. Extract instructions (borrowed from source_markdown)
    const instructions = fleet_config.extractFleetInstructions(source_markdown);

    // 4. Load session checkpoint (or default to fresh)
    const context_json = try loadSessionCheckpoint(alloc, pool, fleet_id_input);
    errdefer alloc.free(context_json);

    const fleet_id = try alloc.dupe(u8, fleet_id_input);
    errdefer alloc.free(fleet_id);

    log.debug("fleet_event_loop_claimed", .{
        .fleet_id = fleet_id,
        .name = config.name,
        .has_checkpoint = context_json.len > 2,
    });

    var session = Self{
        .fleet_id = fleet_id,
        .workspace_id = workspace_id,
        .config = config,
        .instructions = instructions,
        .context_json = context_json,
        .source_markdown = source_markdown,
        .bundle_content_hash = bundle_content_hash,
    };
    // Crash recovery: clear any stale execution_id left by a holder that
    // died mid-stage so the next createExecution starts from a clean slot.
    clearExecutionActive(alloc, &session, pool);
    return session;
}

/// Read the persisted session resume cursor (`core.fleet_sessions.context_json`),
/// or `"{}"` when the fleet has no checkpoint yet. Caller must free the returned slice.
pub fn loadSessionCheckpoint(alloc: Allocator, pool: *pg.Pool, fleet_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT context_json::text FROM core.fleet_sessions WHERE fleet_id = $1
    , .{fleet_id}));
    defer q.deinit();

    if (try q.next()) |row| {
        return alloc.dupe(u8, try row.get([]const u8, 0));
    }
    return alloc.dupe(u8, S_FRESH_CONTEXT);
}

/// Clear active execution in session and DB. Non-fatal — tracking is
/// observability only. Called at claimFleet startup (crash recovery).
pub fn clearExecutionActive(alloc: Allocator, session: *Self, pool: *pg.Pool) void {
    if (session.execution_id) |old| {
        alloc.free(old);
        session.execution_id = null;
    }
    session.execution_started_at = 0;
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    _ = conn.exec(
        \\UPDATE core.fleet_sessions
        \\SET execution_id = NULL, execution_started_at = NULL
        \\WHERE fleet_id = $1::uuid
    , .{session.fleet_id}) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
}

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;
const fleet_config = @import("../fleet_runtime/config.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const log = logging.scoped(.fleet_event_loop);
const S_FRESH_CONTEXT = "{}";
