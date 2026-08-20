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
/// The fleet's own name, from `core.fleets.name` — the instance identity, and
/// what an operator sees. NOT `config.name`: that one is the bundle's declared
/// name from TRIGGER.md, shared by every fleet installed from that bundle. The
/// two diverge whenever a name is server-derived or operator-overridden at
/// create, so anything naming THIS fleet to a human reads this field.
name: []const u8,
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
/// Active execution session handle. NULL when fleet is idle. claimFleet
/// clears a stale persisted handle (crash recovery) via the guarded
/// `CLEAR_STALE_EXECUTION` statement in `fleet/sql.zig`.
execution_id: ?[]const u8 = null,
/// Millis timestamp when execution_id was set. 0 when idle.
execution_started_at: i64 = 0,

comptime {
    const actual = @sizeOf(Self);
    if (actual != 424) @compileError(std.fmt.comptimePrint("FleetSession size changed: {d}, expected 424", .{actual}));
}

pub fn deinit(self: *Self, alloc: Allocator) void {
    alloc.free(self.fleet_id);
    alloc.free(self.workspace_id);
    alloc.free(self.name);
    self.config.deinit(alloc);
    alloc.free(self.source_markdown);
    alloc.free(self.context_json);
    if (self.bundle_content_hash) |bch| alloc.free(bch);
    if (self.execution_id) |eid| alloc.free(eid);
    self.* = undefined;
}

/// Claim a Fleet: fleet row AND session checkpoint in ONE pooled connection
/// and ONE statement (`sql.SELECT_FLEET_WITH_SESSION`) — the pre-merge shape
/// spent three acquires on three single-row reads per claim. Returns a
/// FleetSession that the caller owns and must deinit.
pub fn claimFleet(
    alloc: Allocator,
    fleet_id_input: []const u8,
    pool: *pg.Pool,
) !Self {
    const conn = try pool.acquire();
    defer pool.release(conn);

    // Block-scoped so the read is fully drained before the crash-recovery
    // write below reuses the connection (RULE DRAIN).
    const loaded = blk: {
        var q = PgQuery.from(try conn.query(sql.SELECT_FLEET_WITH_SESSION, .{fleet_id_input}));
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
        errdefer alloc.free(config_json);
        const source_markdown = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(source_markdown);
        // Check status before deinit — row-backed slices are invalid after deinit.
        const status = fleet_config.FleetStatus.fromSlice(try row.get([]const u8, 3)) orelse .stopped;
        // Bundle ref (nullable column): present only for fleets created from a bundle.
        const bundle_content_hash: ?[]const u8 = if (try row.get(?[]const u8, 4)) |bch| try alloc.dupe(u8, bch) else null;
        errdefer if (bundle_content_hash) |bch| alloc.free(bch);
        const name = try alloc.dupe(u8, try row.get([]const u8, 5));
        errdefer alloc.free(name);
        // NULL when the fleet has no checkpoint row yet — fresh session.
        const context_json = if (try row.get(?[]const u8, 6)) |ctx|
            try alloc.dupe(u8, ctx)
        else
            try alloc.dupe(u8, S_FRESH_CONTEXT);
        errdefer alloc.free(context_json);

        if (!status.isRunnable()) {
            log.warn("fleet_event_loop_claim_skipped", .{ .error_code = error_codes.ERR_AGENTSFLEET_PAUSED_INGRESS, .fleet_id = fleet_id_input });
            // The errdefer ladder frees everything duped above.
            return error.FleetNotActive;
        }

        break :blk .{
            .workspace_id = workspace_id,
            .config_json = config_json,
            .source_markdown = source_markdown,
            .bundle_content_hash = bundle_content_hash,
            .name = name,
            .context_json = context_json,
        };
    };
    defer alloc.free(loaded.config_json);
    errdefer {
        alloc.free(loaded.workspace_id);
        alloc.free(loaded.source_markdown);
        if (loaded.bundle_content_hash) |bch| alloc.free(bch);
        alloc.free(loaded.name);
        alloc.free(loaded.context_json);
    }

    var config = try fleet_config.parseStoredFleetConfig(alloc, loaded.config_json);
    errdefer config.deinit(alloc);

    // Instructions borrow from source_markdown, which the session owns.
    const instructions = fleet_config.extractFleetInstructions(loaded.source_markdown);

    const fleet_id = try alloc.dupe(u8, fleet_id_input);
    errdefer alloc.free(fleet_id);

    log.debug("fleet_event_loop_claimed", .{
        .fleet_id = fleet_id,
        .name = config.name,
        .has_checkpoint = loaded.context_json.len > 2,
    });

    // Crash recovery on the SAME connection: clear an execution handle a dead
    // holder left behind. The statement's IS NOT NULL guard makes this a
    // zero-row no-op on the steady state. Non-fatal — tracking is
    // observability only.
    _ = conn.exec(sql.CLEAR_STALE_EXECUTION, .{fleet_id_input}) catch |err|
        log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });

    return .{
        .fleet_id = fleet_id,
        .workspace_id = loaded.workspace_id,
        .name = loaded.name,
        .config = config,
        .instructions = instructions,
        .context_json = loaded.context_json,
        .source_markdown = loaded.source_markdown,
        .bundle_content_hash = loaded.bundle_content_hash,
    };
}

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("sql.zig");
const Allocator = std.mem.Allocator;
const fleet_config = @import("../fleet_runtime/config.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const log = logging.scoped(.fleet_event_loop);
const S_FRESH_CONTEXT = "{}";
