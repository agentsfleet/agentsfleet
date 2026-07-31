//! Declarative Fleet cron synchronization.
//!
//! This adapter is intentionally synchronous: the user-facing Fleet write either
//! applies the QStash mutation now or returns an explicit schedule error. There
//! is no local cron fallback, resident repair loop, or background poller here.

const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const cron_constants = @import("../../../cron/constants.zig");
const sql = @import("sql.zig");
const cron_model = @import("../../../cron/model.zig");
const QStashClient = @import("../../../cron/QStashClient.zig");
const Service = @import("../../../cron/Service.zig");
const Store = @import("../../../cron/Store.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const Hx = @import("../hx.zig").Hx;

const log = logging.scoped(.fleet_cron_sync);

const SOURCE_KEY = "trigger:cron";
/// The reconciliation record for a schedule this process could not retire.
/// Emitted per schedule and BEFORE teardown deletes the rows, because the
/// identifier a human needs to delete it at the provider exists nowhere else
/// afterwards.
const LOG_UNREGISTER_FAILED = "schedule_unregister_failed";
const LOG_UNREGISTER_UNCONFIGURED = "schedule_unregister_unconfigured";
const LOG_UNREGISTER_UNREADABLE = "schedule_unregister_unreadable";
const DETAIL_BUSY = "Declarative cron synchronization lease is busy";
const DETAIL_INVALID = "Declarative cron trigger is invalid";
const DETAIL_OPERATION = "Declarative cron synchronization failed";
const DETAIL_PROVIDER = "QStash did not confirm declarative cron synchronization";
const DETAIL_UNCONFIGURED = "QStash credentials are not configured";
const STATE_SYNCING = "syncing";

pub const Result = enum {
    ok,
    skipped,
    busy,
    invalid,
    provider_failed,
    unconfigured,
    not_found,
    internal,
};

pub fn syncNewConfig(hx: Hx, fleet_id: []const u8, config: fleet_config.FleetConfig) Result {
    const trigger = firstCron(config.triggers) orelse return .skipped;
    return applySource(hx, fleet_id, trigger, .active);
}

pub fn syncParsedConfig(hx: Hx, fleet_id: []const u8, config: fleet_config.FleetConfig, desired: cron_model.DesiredStatus) Result {
    const trigger = firstCron(config.triggers) orelse {
        return removeSource(hx, fleet_id, .missing_ok);
    };
    return applySource(hx, fleet_id, trigger, desired);
}

pub fn syncStoredFleet(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) Result {
    const stored = readStoredFleet(hx, workspace_id, fleet_id) catch {
        return .internal;
    } orelse return .not_found;
    defer hx.alloc.free(stored.config_json);
    var config = fleet_config.parseFleetConfig(hx.alloc, stored.config_json) catch {
        return .invalid;
    };
    defer config.deinit(hx.alloc);
    return switch (stored.status) {
        .active, .installing => syncParsedConfig(hx, fleet_id, config, .active),
        .paused, .stopped, .killed => pauseSource(hx, fleet_id),
    };
}

/// Unregister every schedule the fleet owns, upstream.
///
/// EVERY schedule is attempted, including after one fails. The only caller is
/// account teardown, which is about to delete the rows that name these
/// schedules: a schedule skipped because an earlier sibling failed becomes a
/// timer that fires forever with nothing left on disk to reconcile it against.
/// Stopping at the first failure turned one transient provider error into a
/// permanent leak of every schedule behind it.
///
/// The returned `Result` is the first failure seen, so a caller still learns
/// that something went wrong; the per-schedule identifiers a human needs to
/// retire the survivors by hand ride the log lines below, emitted BEFORE the
/// purge erases them.
///
/// `.skipped` for an empty list is decided before credentials are resolved, so
/// an `.unconfigured` from this function always means the stronger thing:
/// schedules existed and NONE of them were retired.
pub fn removeAll(hx: Hx, fleet_id: []const u8) Result {
    const schedules = Store.init(hx.ctx.pool).list(hx.alloc, fleet_id) catch |err| {
        // The enumeration itself failed, so this process never learned which
        // schedules exist — and teardown erases the rows moments later. Nothing
        // downstream can name them, which is why the failure is logged with its
        // own event and its own cause rather than reaching the caller as a bare
        // `.internal` it would misreport as a provider fault.
        log.warn(LOG_UNREGISTER_UNREADABLE, .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .fleet_id = fleet_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
        return .internal;
    };
    defer {
        for (schedules) |*schedule| schedule.deinit(hx.alloc);
        hx.alloc.free(schedules);
    }
    if (schedules.len == 0) return .skipped;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = serviceFromContext(hx, &exchange, &destination_buffer) orelse {
        // Every schedule leaks at once here, so every identifier is emitted —
        // a count would say how many timers were abandoned without saying which,
        // and the rows that could answer that are about to be deleted. The ids
        // are in hand; the only reason not to write them down is haste.
        for (schedules) |schedule| {
            log.warn(LOG_UNREGISTER_UNCONFIGURED, .{
                .error_code = ec.ERR_SCHEDULE_NOT_CONFIGURED,
                .fleet_id = fleet_id,
                .schedule_id = schedule.schedule_id,
                .schedules = schedules.len,
                .req_id = hx.req_id,
            });
        }
        return .unconfigured;
    };
    var first_failure: ?Result = null;
    for (schedules) |schedule| {
        const result = removeOne(hx, service, fleet_id, schedule.schedule_id);
        if (result == .ok or result == .skipped) continue;
        log.warn(LOG_UNREGISTER_FAILED, .{
            .error_code = ec.ERR_SCHEDULE_PROVIDER_UNAVAILABLE,
            .fleet_id = fleet_id,
            .schedule_id = schedule.schedule_id,
            .result = @tagName(result),
            .req_id = hx.req_id,
        });
        if (first_failure == null) first_failure = result;
    }
    return first_failure orelse .ok;
}

fn removeOne(hx: Hx, service: Service, fleet_id: []const u8, schedule_id: []const u8) Result {
    var outcome = service.remove(hx.alloc, fleet_id, schedule_id) catch return .internal;
    defer outcome.deinit(hx.alloc);
    return mapOutcome(outcome, .missing_ok);
}

pub fn writeFailure(hx: Hx, result: Result) bool {
    switch (result) {
        .ok, .skipped => return true,
        .busy => common.errorResponseConflict(hx.res, ec.ERR_SCHEDULE_UPDATE_BUSY, DETAIL_BUSY, hx.req_id, STATE_SYNCING),
        .invalid => hx.fail(ec.ERR_SCHEDULE_INVALID, DETAIL_INVALID),
        .provider_failed => hx.fail(ec.ERR_SCHEDULE_PROVIDER_UNAVAILABLE, DETAIL_PROVIDER),
        .unconfigured => hx.fail(ec.ERR_SCHEDULE_NOT_CONFIGURED, DETAIL_UNCONFIGURED),
        .not_found => hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND),
        .internal => hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, DETAIL_OPERATION),
    }
    return false;
}

fn applySource(hx: Hx, fleet_id: []const u8, trigger: CronTrigger, desired: cron_model.DesiredStatus) Result {
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = serviceFromContext(hx, &exchange, &destination_buffer) orelse return .unconfigured;
    var outcome = service.upsertSource(hx.alloc, .{
        .fleet_id = fleet_id,
        .source = .trigger,
        .source_key = SOURCE_KEY,
        .cron = trigger.schedule,
        .timezone = trigger.timezone,
        .message = trigger.message,
        .desired_status = desired,
    }) catch |err| return mapError(err);
    defer outcome.deinit(hx.alloc);
    return mapOutcome(outcome, .strict);
}

fn pauseSource(hx: Hx, fleet_id: []const u8) Result {
    if (!(sourceScheduleExists(hx, fleet_id) catch return .internal)) return .skipped;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = serviceFromContext(hx, &exchange, &destination_buffer) orelse return .unconfigured;
    var outcome = service.setSourceDesired(hx.alloc, fleet_id, SOURCE_KEY, .paused) catch |err| return mapError(err);
    defer outcome.deinit(hx.alloc);
    return mapOutcome(outcome, .missing_ok);
}

fn removeSource(hx: Hx, fleet_id: []const u8, missing: MissingPolicy) Result {
    if (missing == .missing_ok and !(sourceScheduleExists(hx, fleet_id) catch return .internal)) return .skipped;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = serviceFromContext(hx, &exchange, &destination_buffer) orelse return .unconfigured;
    var outcome = service.removeSource(hx.alloc, fleet_id, SOURCE_KEY) catch |err| return mapError(err);
    defer outcome.deinit(hx.alloc);
    return mapOutcome(outcome, missing);
}

fn sourceScheduleExists(hx: Hx, fleet_id: []const u8) !bool {
    var schedule = try Store.init(hx.ctx.pool).getBySourceKey(hx.alloc, fleet_id, SOURCE_KEY);
    defer if (schedule) |*value| value.deinit(hx.alloc);
    return schedule != null;
}

fn serviceFromContext(
    hx: Hx,
    exchange: *QStashClient.HttpClientExchange,
    destination_buffer: *[cron_constants.max_destination_url_bytes]u8,
) ?Service {
    const credentials = hx.ctx.qstash_credentials orelse return null;
    const destination = cron_constants.destinationUrl(destination_buffer, hx.ctx.api_url) catch return null;
    const qstash = if (hx.ctx.qstash_exchange_override) |override|
        QStashClient.init(override, credentials.url, destination)
    else blk: {
        exchange.* = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
        break :blk QStashClient.init(exchange.exchange(), credentials.url, destination);
    };
    return Service.init(Store.init(hx.ctx.pool), qstash, credentials.token);
}

const StoredFleet = struct { config_json: []const u8, status: fleet_config.FleetStatus };

fn readStoredFleet(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) !?StoredFleet {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    var query = PgQuery.from(try conn.query(sql.SELECT_FLEET_CONFIG_AND_STATUS, .{ fleet_id, workspace_id }));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    const status = fleet_config.FleetStatus.fromSlice(try row.get([]const u8, 1)) orelse return error.InvalidFleetStatus;
    return .{ .config_json = try hx.alloc.dupe(u8, try row.get([]const u8, 0)), .status = status };
}

const CronTrigger = struct {
    schedule: []const u8,
    timezone: []const u8,
    message: []const u8,
};

fn firstCron(triggers: []const fleet_config.FleetTrigger) ?CronTrigger {
    for (triggers) |trigger| switch (trigger) {
        .cron => |cron| return .{
            .schedule = cron.schedule,
            .timezone = cron.timezone,
            .message = cron.message,
        },
        .webhook, .api => {},
    };
    return null;
}

const MissingPolicy = enum { strict, missing_ok };

fn mapOutcome(outcome: Service.Outcome, missing: MissingPolicy) Result {
    return switch (outcome) {
        .schedule, .deleted => .ok,
        .not_found => if (missing == .missing_ok) .skipped else .not_found,
        .provider_failed => .provider_failed,
        .busy => .busy,
        .fleet_not_found => .not_found,
        .cap_reached, .source_conflict => .busy,
    };
}

fn mapError(err: anyerror) Result {
    return switch (err) {
        error.InvalidCron, error.InvalidTimezone, error.InvalidMessage => .invalid,
        else => .internal,
    };
}
