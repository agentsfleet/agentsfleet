//! Synchronous schedule mutation service.
//!
//! Database claims and finalization are short operations. QStash I/O always
//! runs after the store releases its connection; no transaction crosses the
//! provider boundary and no background reader repairs failed state.

const Service = @This();

const std = @import("std");
const common = @import("common");

const QStashClient = @import("QStashClient.zig");
const Store = @import("Store.zig");
const id_format = @import("../types/id_format.zig");
const model = @import("model.zig");
const validate = @import("validate.zig");

pub const SYNC_LEASE_MS: i64 = 15_000;

const DETAIL_INVALID = "QStash rejected the schedule parameters";
const DETAIL_RATE_LIMITED = "QStash rate limited the schedule mutation";
const DETAIL_UNAVAILABLE = "QStash schedule mutation did not complete";
const DETAIL_MALFORMED = "QStash returned an invalid schedule response";
const DETAIL_OUT_OF_MEMORY = "Schedule synchronization exhausted local memory";

store: Store,
qstash: QStashClient,
qstash_token: []const u8,

pub const UpdateInput = struct {
    fleet_id: []const u8,
    schedule_id: []const u8,
    cron: []const u8,
    timezone: []const u8,
    message: []const u8,
    desired_status: model.DesiredStatus,
};

pub const SourceInput = struct {
    fleet_id: []const u8,
    source: model.Source,
    source_key: []const u8,
    cron: []const u8,
    timezone: []const u8,
    message: []const u8,
    desired_status: model.DesiredStatus,
};

pub const ProviderFailure = struct {
    schedule: model.Schedule,
    cause: QStashClient.Outcome,
};

pub const Outcome = union(enum) {
    schedule: model.Schedule,
    provider_failed: ProviderFailure,
    deleted,
    busy,
    not_found,
    fleet_not_found,
    cap_reached,
    source_conflict,

    pub fn deinit(self: *Outcome, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .schedule => |*schedule| schedule.deinit(alloc),
            .provider_failed => |*failure| failure.schedule.deinit(alloc),
            else => {},
        }
        self.* = undefined;
    }
};

pub fn init(store: Store, qstash: QStashClient, qstash_token: []const u8) Service {
    return .{ .store = store, .qstash = qstash, .qstash_token = qstash_token };
}

pub fn create(self: Service, alloc: std.mem.Allocator, input: model.CreateInput) !Outcome {
    try validateInput(input.cron, input.timezone, input.message);
    const schedule_id = try id_format.generateScheduleId(alloc);
    defer alloc.free(schedule_id);
    const lease_token = try id_format.allocUuidV7(alloc);
    defer alloc.free(lease_token);
    const now_ms = common.clock.nowMillis();
    const created = try self.store.create(
        alloc,
        input,
        schedule_id,
        lease_token,
        now_ms,
        now_ms + SYNC_LEASE_MS,
    );
    return switch (created) {
        .created => |schedule| self.applyClaimed(alloc, schedule),
        .fleet_not_found => .fleet_not_found,
        .cap_reached => .cap_reached,
        .source_conflict => .source_conflict,
    };
}

pub fn update(self: Service, alloc: std.mem.Allocator, input: UpdateInput) !Outcome {
    try validateInput(input.cron, input.timezone, input.message);
    return self.claimAndApply(alloc, input);
}

pub fn sync(self: Service, alloc: std.mem.Allocator, fleet_id: []const u8, schedule_id: []const u8) !Outcome {
    var schedule = (try self.store.get(alloc, fleet_id, schedule_id)) orelse return .not_found;
    defer schedule.deinit(alloc);
    return self.claimAndApply(alloc, .{
        .fleet_id = schedule.fleet_id,
        .schedule_id = schedule.schedule_id,
        .cron = schedule.cron,
        .timezone = schedule.timezone,
        .message = schedule.message,
        .desired_status = schedule.desired_status,
    });
}

pub fn remove(self: Service, alloc: std.mem.Allocator, fleet_id: []const u8, schedule_id: []const u8) !Outcome {
    var schedule = (try self.store.get(alloc, fleet_id, schedule_id)) orelse return .not_found;
    defer schedule.deinit(alloc);
    return self.claimAndApply(alloc, .{
        .fleet_id = schedule.fleet_id,
        .schedule_id = schedule.schedule_id,
        .cron = schedule.cron,
        .timezone = schedule.timezone,
        .message = schedule.message,
        .desired_status = .deleting,
    });
}

pub fn upsertSource(self: Service, alloc: std.mem.Allocator, input: SourceInput) !Outcome {
    try validateInput(input.cron, input.timezone, input.message);
    var existing = (try self.store.getBySourceKey(alloc, input.fleet_id, input.source_key)) orelse {
        if (input.desired_status != .active) return .not_found;
        return self.create(alloc, .{
            .fleet_id = input.fleet_id,
            .source = input.source,
            .source_key = input.source_key,
            .cron = input.cron,
            .timezone = input.timezone,
            .message = input.message,
        });
    };
    defer existing.deinit(alloc);
    return self.claimAndApply(alloc, .{
        .fleet_id = existing.fleet_id,
        .schedule_id = existing.schedule_id,
        .cron = input.cron,
        .timezone = input.timezone,
        .message = input.message,
        .desired_status = input.desired_status,
    });
}

pub fn setSourceDesired(
    self: Service,
    alloc: std.mem.Allocator,
    fleet_id: []const u8,
    source_key: []const u8,
    desired_status: model.DesiredStatus,
) !Outcome {
    var existing = (try self.store.getBySourceKey(alloc, fleet_id, source_key)) orelse return .not_found;
    defer existing.deinit(alloc);
    return self.claimAndApply(alloc, .{
        .fleet_id = existing.fleet_id,
        .schedule_id = existing.schedule_id,
        .cron = existing.cron,
        .timezone = existing.timezone,
        .message = existing.message,
        .desired_status = desired_status,
    });
}

pub fn removeSource(self: Service, alloc: std.mem.Allocator, fleet_id: []const u8, source_key: []const u8) !Outcome {
    var existing = (try self.store.getBySourceKey(alloc, fleet_id, source_key)) orelse return .not_found;
    defer existing.deinit(alloc);
    return self.claimAndApply(alloc, .{
        .fleet_id = existing.fleet_id,
        .schedule_id = existing.schedule_id,
        .cron = existing.cron,
        .timezone = existing.timezone,
        .message = existing.message,
        .desired_status = .deleting,
    });
}

fn claimAndApply(self: Service, alloc: std.mem.Allocator, input: UpdateInput) !Outcome {
    const lease_token = try id_format.allocUuidV7(alloc);
    defer alloc.free(lease_token);
    const now_ms = common.clock.nowMillis();
    const claimed = try self.store.claimMutation(alloc, .{
        .schedule_id = input.schedule_id,
        .fleet_id = input.fleet_id,
        .cron = input.cron,
        .timezone = input.timezone,
        .message = input.message,
        .desired_status = input.desired_status,
        .lease_token = lease_token,
        .now_ms = now_ms,
        .lease_until_ms = now_ms + SYNC_LEASE_MS,
    });
    return switch (claimed) {
        .claimed => |schedule| self.applyClaimed(alloc, schedule),
        .busy => .busy,
        .not_found => .not_found,
    };
}

fn applyClaimed(self: Service, alloc: std.mem.Allocator, owned: model.Schedule) !Outcome {
    var schedule = owned;
    defer schedule.deinit(alloc);
    const token = schedule.sync_token orelse return error.MissingSyncToken;
    const provider = switch (schedule.desired_status) {
        .active => self.qstash.upsert(alloc, self.qstash_token, schedule),
        .paused, .deleting => self.qstash.delete(alloc, self.qstash_token, schedule.schedule_id),
    } catch |err| {
        _ = try self.store.finalizeFailureState(
            schedule.schedule_id,
            schedule.generation,
            token,
            DETAIL_OUT_OF_MEMORY,
            common.clock.nowMillis(),
        );
        return err;
    };
    if (provider != .success) return self.finishFailure(alloc, schedule, token, provider);
    if (schedule.desired_status == .deleting) {
        if (!try self.store.deleteClaimed(schedule.schedule_id, schedule.generation, token)) return error.MutationLost;
        return .deleted;
    }
    return self.finishSuccess(alloc, schedule, token);
}

fn finishSuccess(self: Service, alloc: std.mem.Allocator, schedule: model.Schedule, token: []const u8) !Outcome {
    const finalized = self.store.finalizeSuccess(
        alloc,
        schedule.schedule_id,
        schedule.generation,
        token,
        common.clock.nowMillis(),
    ) catch |err| {
        _ = try self.store.finalizeSuccessState(schedule.schedule_id, schedule.generation, token, common.clock.nowMillis());
        return err;
    };
    return .{ .schedule = finalized orelse return error.MutationLost };
}

fn finishFailure(
    self: Service,
    alloc: std.mem.Allocator,
    schedule: model.Schedule,
    token: []const u8,
    cause: QStashClient.Outcome,
) !Outcome {
    const detail = failureDetail(cause);
    const finalized = self.store.finalizeFailure(
        alloc,
        schedule.schedule_id,
        schedule.generation,
        token,
        detail,
        common.clock.nowMillis(),
    ) catch |err| {
        _ = try self.store.finalizeFailureState(schedule.schedule_id, schedule.generation, token, detail, common.clock.nowMillis());
        return err;
    };
    return .{ .provider_failed = .{
        .schedule = finalized orelse return error.MutationLost,
        .cause = cause,
    } };
}

fn failureDetail(cause: QStashClient.Outcome) []const u8 {
    return switch (cause) {
        .invalid_request => DETAIL_INVALID,
        .rate_limited => DETAIL_RATE_LIMITED,
        .unavailable => DETAIL_UNAVAILABLE,
        .malformed_response => DETAIL_MALFORMED,
        .success => unreachable,
    };
}

fn validateInput(cron: []const u8, timezone: []const u8, message: []const u8) !void {
    try validate.cron(cron);
    try validate.timezone(timezone);
    try validate.message(message);
}
