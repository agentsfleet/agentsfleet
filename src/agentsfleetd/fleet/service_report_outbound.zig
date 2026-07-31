//! Connector-outbound answer delivery for the report path, split from
//! `service_report.zig` for the file-length budget. The report finalizer hands
//! the answer here; nothing in this module can fail the already-finalized
//! report (every path is a logged no-op).

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const connector_outbound = @import("../queue/connector_outbound.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_report);

/// The three identifiers delivery needs — the caller's lease row shape stays
/// private to the report module.
pub const EventRef = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
};

const SELECT_BOUND_PROVIDER_SQL =
    \\SELECT provider FROM core.connector_channels WHERE fleet_id = $1::uuid LIMIT 1
;

/// If the reporting fleet has a `connector_channels` binding, enqueue the answer
/// for out-of-band delivery on the generic `connector:outbound` stream. Most
/// fleets are not connector-resident, so this is a common-case miss served by the
/// `connector_channels(fleet_id)` index (migration 032). Best-effort +
/// provider-agnostic: an empty answer, a miss, or any failure is a logged no-op
/// — it never fails the already-finalized report, and it imports no connector
/// (it enqueues a provider-tagged generic job the worker routes).
pub fn enqueueOutboundAnswer(hx: Hx, ref: EventRef, answer: []const u8) void {
    if (answer.len == 0) return; // a crashed / empty run has nothing to deliver
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    const provider = lookupBoundProvider(hx.alloc, conn, ref.fleet_id) catch |err| {
        log.warn("outbound_binding_lookup_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = ref.fleet_id, .err = @errorName(err) });
        return;
    } orelse return; // not a connector fleet — the common case
    defer hx.alloc.free(provider);
    const entry_id = connector_outbound.enqueue(hx.ctx.queue, .{
        .provider = provider,
        .workspace_id = ref.workspace_id,
        .fleet_id = ref.fleet_id,
        .event_id = ref.event_id,
        .answer = answer,
    }) catch |err| {
        log.warn("outbound_enqueue_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = ref.fleet_id, .err = @errorName(err) });
        return;
    };
    hx.ctx.alloc.free(entry_id);
    log.debug("outbound_answer_enqueued", .{ .fleet_id = ref.fleet_id, .provider = provider });
}

/// Generic reverse lookup: `fleet_id → provider` if the fleet has any connector
/// binding. Returns an owned provider (caller frees) or null. Provider is an
/// opaque string — the report path never learns which connector.
fn lookupBoundProvider(alloc: std.mem.Allocator, conn: *pg.Conn, fleet_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(SELECT_BOUND_PROVIDER_SQL, .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}
