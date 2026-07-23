//! Schedule management REST adapters.

const std = @import("std");
const httpz = @import("httpz");

const cron_constants = @import("../../../cron/constants.zig");
const cron_model = @import("../../../cron/model.zig");
const QStashClient = @import("../../../cron/QStashClient.zig");
const Service = @import("../../../cron/Service.zig");
const Store = @import("../../../cron/Store.zig");
const common = @import("../common.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const workspace_guards = @import("../../workspace_guards.zig");
const Hx = @import("../hx.zig").Hx;

const DETAIL_INVALID = "cron, timezone, or message is invalid";
const DETAIL_NOT_FOUND = "Schedule not found";
const DETAIL_PROVIDER = "QStash did not confirm the schedule change; run schedule sync.";
const DETAIL_BUSY = "Schedule synchronization lease is busy";
const DETAIL_SOURCE_CONFLICT = "A schedule with this source key already exists";
const DETAIL_LIMIT = "Fleet schedule limit reached";
const DETAIL_UNCONFIGURED = "QStash credentials are not configured";
const DETAIL_WORKSPACE = "Workspace access denied";
const DETAIL_STATUS = "desired_status must be active or paused";
const DETAIL_OPERATION = "Schedule operation failed";
const SOURCE_KEY_FORMAT = "api:{s}";
const STATE_SYNCING = "syncing";

const CreateBody = struct {
    cron: []const u8,
    timezone: ?[]const u8 = null,
    message: []const u8,
};

const PatchBody = struct {
    cron: ?[]const u8 = null,
    timezone: ?[]const u8 = null,
    message: ?[]const u8 = null,
    desired_status: ?[]const u8 = null,
};

const ScheduleJson = struct {
    schedule_id: []const u8,
    fleet_id: []const u8,
    source: []const u8,
    cron: []const u8,
    timezone: []const u8,
    message: []const u8,
    desired_status: []const u8,
    sync_status: []const u8,
    generation: i64,
    last_error: ?[]const u8,
    created_at: i64,
    updated_at: i64,
};

pub fn innerScheduleCollection(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    switch (req.method) {
        .GET => listSchedules(hx, workspace_id, fleet_id),
        .POST => createSchedule(hx, req, workspace_id, fleet_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn innerScheduleItem(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8, schedule_id: []const u8) void {
    switch (req.method) {
        .GET => getSchedule(hx, workspace_id, fleet_id, schedule_id),
        .PATCH => patchSchedule(hx, req, workspace_id, fleet_id, schedule_id),
        .DELETE => deleteSchedule(hx, workspace_id, fleet_id, schedule_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn innerScheduleSync(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8, schedule_id: []const u8) void {
    if (!common.requireMethod(hx.res, req.method, .POST)) return;
    if (!validateIds(hx, workspace_id, fleet_id, schedule_id)) return;
    if (!authorizeMutationFleet(hx, workspace_id, fleet_id)) return;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = cronService(hx, &exchange, &destination_buffer) orelse return;
    var outcome = service.sync(hx.alloc, fleet_id, schedule_id) catch |err| return serviceError(hx, err);
    defer outcome.deinit(hx.alloc);
    writeOutcome(hx, outcome, .ok);
}

fn listSchedules(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!validateIds(hx, workspace_id, fleet_id, null)) return;
    if (!authorizeReadFleet(hx, workspace_id, fleet_id)) return;
    const store = Store.init(hx.ctx.pool);
    const schedules = store.list(hx.alloc, fleet_id) catch |err| return serviceError(hx, err);
    defer {
        for (schedules) |*schedule| schedule.deinit(hx.alloc);
        hx.alloc.free(schedules);
    }
    const items = hx.alloc.alloc(ScheduleJson, schedules.len) catch return common.internalDbError(hx.res, hx.req_id);
    defer hx.alloc.free(items);
    for (schedules, 0..) |schedule, index| items[index] = scheduleJson(schedule);
    hx.ok(.ok, .{ .items = items, .total = items.len, .next_cursor = @as(?[]const u8, null) });
}

fn getSchedule(hx: Hx, workspace_id: []const u8, fleet_id: []const u8, schedule_id: []const u8) void {
    if (!validateIds(hx, workspace_id, fleet_id, schedule_id)) return;
    if (!authorizeReadFleet(hx, workspace_id, fleet_id)) return;
    const store = Store.init(hx.ctx.pool);
    var schedule = (store.get(hx.alloc, fleet_id, schedule_id) catch |err| return serviceError(hx, err)) orelse {
        hx.fail(error_codes.ERR_SCHEDULE_NOT_FOUND, DETAIL_NOT_FOUND);
        return;
    };
    defer schedule.deinit(hx.alloc);
    hx.ok(.ok, scheduleJson(schedule));
}

fn createSchedule(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!validateIds(hx, workspace_id, fleet_id, null)) return;
    if (!authorizeMutationFleet(hx, workspace_id, fleet_id)) return;
    var parsed = parseBody(CreateBody, hx, req) orelse return;
    defer parsed.deinit();
    var source_key_buffer: [64]u8 = undefined;
    const source_key = std.fmt.bufPrint(&source_key_buffer, SOURCE_KEY_FORMAT, .{hx.req_id}) catch return common.internalDbError(hx.res, hx.req_id);
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = cronService(hx, &exchange, &destination_buffer) orelse return;
    var outcome = service.create(hx.alloc, .{
        .fleet_id = fleet_id,
        .source = .api,
        .source_key = source_key,
        .cron = parsed.value.cron,
        .timezone = parsed.value.timezone orelse cron_model.DEFAULT_TIMEZONE,
        .message = parsed.value.message,
    }) catch |err| return serviceError(hx, err);
    defer outcome.deinit(hx.alloc);
    writeOutcome(hx, outcome, .created);
}

fn patchSchedule(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8, schedule_id: []const u8) void {
    if (!validateIds(hx, workspace_id, fleet_id, schedule_id)) return;
    if (!authorizeMutationFleet(hx, workspace_id, fleet_id)) return;
    var parsed = parseBody(PatchBody, hx, req) orelse return;
    defer parsed.deinit();
    const desired = if (parsed.value.desired_status) |raw| parseDesiredStatus(raw) orelse {
        hx.fail(error_codes.ERR_SCHEDULE_INVALID, DETAIL_STATUS);
        return;
    } else null;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = cronService(hx, &exchange, &destination_buffer) orelse return;
    var outcome = service.update(hx.alloc, .{
        .fleet_id = fleet_id,
        .schedule_id = schedule_id,
        .cron = parsed.value.cron,
        .timezone = parsed.value.timezone,
        .message = parsed.value.message,
        .desired_status = desired,
    }) catch |err| return serviceError(hx, err);
    defer outcome.deinit(hx.alloc);
    writeOutcome(hx, outcome, .ok);
}

fn deleteSchedule(hx: Hx, workspace_id: []const u8, fleet_id: []const u8, schedule_id: []const u8) void {
    if (!validateIds(hx, workspace_id, fleet_id, schedule_id)) return;
    if (!authorizeMutationFleet(hx, workspace_id, fleet_id)) return;
    var exchange: QStashClient.HttpClientExchange = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const service = cronService(hx, &exchange, &destination_buffer) orelse return;
    var outcome = service.remove(hx.alloc, fleet_id, schedule_id) catch |err| return serviceError(hx, err);
    defer outcome.deinit(hx.alloc);
    writeOutcome(hx, outcome, .no_content);
}

fn cronService(
    hx: Hx,
    exchange: *QStashClient.HttpClientExchange,
    destination_buffer: *[cron_constants.max_destination_url_bytes]u8,
) ?Service {
    const credentials = hx.ctx.qstash_credentials orelse {
        hx.fail(error_codes.ERR_SCHEDULE_NOT_CONFIGURED, DETAIL_UNCONFIGURED);
        return null;
    };
    const destination = cron_constants.destinationUrl(destination_buffer, hx.ctx.api_url) catch {
        hx.fail(error_codes.ERR_SCHEDULE_NOT_CONFIGURED, DETAIL_UNCONFIGURED);
        return null;
    };
    const qstash = if (hx.ctx.qstash_exchange_override) |override|
        QStashClient.init(override, credentials.url, destination)
    else blk: {
        exchange.* = .{ .io = hx.ctx.io, .sched = hx.ctx.deadline_scheduler };
        break :blk QStashClient.init(exchange.exchange(), credentials.url, destination);
    };
    return Service.init(Store.init(hx.ctx.pool), qstash, credentials.token);
}

fn authorizeRead(hx: Hx, workspace_id: []const u8) bool {
    var db = hx.db() orelse return false;
    defer db.end();
    if (common.authorizeWorkspace(db.conn, hx.principal, workspace_id)) return true;
    hx.fail(error_codes.ERR_FORBIDDEN, DETAIL_WORKSPACE);
    return false;
}

fn authorizeMutation(hx: Hx, workspace_id: []const u8) bool {
    var db = hx.db() orelse return false;
    defer db.end();
    const access = workspace_guards.enforce(hx.res, hx.req_id, db.conn, hx.principal, workspace_id) orelse return false;
    access.deinit(hx.alloc);
    return true;
}

fn authorizeReadFleet(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) bool {
    return authorizeRead(hx, workspace_id) and requireFleetInWorkspace(hx, workspace_id, fleet_id);
}

fn authorizeMutationFleet(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) bool {
    return authorizeMutation(hx, workspace_id) and requireFleetInWorkspace(hx, workspace_id, fleet_id);
}

fn requireFleetInWorkspace(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) bool {
    const store = Store.init(hx.ctx.pool);
    const belongs = store.fleetBelongsToWorkspace(fleet_id, workspace_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    if (belongs) return true;
    hx.fail(error_codes.ERR_AGENTSFLEET_NOT_FOUND, error_codes.MSG_AGENTSFLEET_NOT_FOUND);
    return false;
}

fn parseBody(comptime Body: type, hx: Hx, req: *httpz.Request) ?std.json.Parsed(Body) {
    const raw = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, error_codes.MSG_MALFORMED_JSON);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, raw, hx.req_id)) return null;
    return std.json.parseFromSlice(Body, hx.alloc, raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, error_codes.MSG_MALFORMED_JSON);
        return null;
    };
}

fn validateIds(hx: Hx, workspace_id: []const u8, fleet_id: []const u8, schedule_id: ?[]const u8) bool {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) return failInvalidId(hx, "workspace_id");
    if (!id_format.isSupportedWorkspaceId(fleet_id)) return failInvalidId(hx, "fleet_id");
    if (schedule_id) |sid| if (!id_format.isSupportedWorkspaceId(sid)) return failInvalidId(hx, "schedule_id");
    return true;
}

fn failInvalidId(hx: Hx, label: []const u8) bool {
    var buffer: [96]u8 = undefined;
    const detail = std.fmt.bufPrint(&buffer, "{s} must be a valid UUIDv7", .{label}) catch "identifier must be a valid UUIDv7";
    hx.fail(error_codes.ERR_INVALID_REQUEST, detail);
    return false;
}

fn parseDesiredStatus(value: []const u8) ?cron_model.DesiredStatus {
    const status = cron_model.DesiredStatus.fromSlice(value) orelse return null;
    return switch (status) {
        .active, .paused => status,
        .deleting => null,
    };
}

fn writeOutcome(hx: Hx, outcome: Service.Outcome, success_status: std.http.Status) void {
    switch (outcome) {
        .schedule => |schedule| hx.ok(success_status, scheduleJson(schedule)),
        .deleted => hx.noContent(),
        .provider_failed => hx.fail(error_codes.ERR_SCHEDULE_PROVIDER_UNAVAILABLE, DETAIL_PROVIDER),
        .busy => common.errorResponseConflict(hx.res, error_codes.ERR_SCHEDULE_UPDATE_BUSY, DETAIL_BUSY, hx.req_id, STATE_SYNCING),
        .not_found => hx.fail(error_codes.ERR_SCHEDULE_NOT_FOUND, DETAIL_NOT_FOUND),
        .fleet_not_found => hx.fail(error_codes.ERR_AGENTSFLEET_NOT_FOUND, error_codes.MSG_AGENTSFLEET_NOT_FOUND),
        .cap_reached => common.errorResponseConflict(hx.res, error_codes.ERR_SCHEDULE_LIMIT_REACHED, DETAIL_LIMIT, hx.req_id, "limit_reached"),
        .source_conflict => common.errorResponseConflict(hx.res, error_codes.ERR_SCHEDULE_CONFLICT, DETAIL_SOURCE_CONFLICT, hx.req_id, "source_conflict"),
    }
}

fn serviceError(hx: Hx, err: anyerror) void {
    switch (err) {
        error.InvalidCron, error.InvalidTimezone, error.InvalidMessage => hx.fail(error_codes.ERR_SCHEDULE_INVALID, DETAIL_INVALID),
        else => hx.fail(error_codes.ERR_INTERNAL_OPERATION_FAILED, DETAIL_OPERATION),
    }
}

fn scheduleJson(schedule: cron_model.Schedule) ScheduleJson {
    return .{
        .schedule_id = schedule.schedule_id,
        .fleet_id = schedule.fleet_id,
        .source = schedule.source.toSlice(),
        .cron = schedule.cron,
        .timezone = schedule.timezone,
        .message = schedule.message,
        .desired_status = schedule.desired_status.toSlice(),
        .sync_status = schedule.sync_status.toSlice(),
        .generation = schedule.generation,
        .last_error = schedule.last_error,
        .created_at = schedule.created_at,
        .updated_at = schedule.updated_at,
    };
}
