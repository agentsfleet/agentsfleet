//! Schedule value types. Strings on `Schedule` are caller-allocator owned.

const std = @import("std");

pub const MAX_SCHEDULES_PER_FLEET: usize = 32;
pub const DEFAULT_TIMEZONE = "UTC";

pub const Source = enum {
    api,
    trigger,

    pub fn toSlice(self: Source) []const u8 {
        return @tagName(self);
    }

    pub fn fromSlice(value: []const u8) ?Source {
        if (std.mem.eql(u8, value, "api")) return .api;
        if (std.mem.eql(u8, value, "trigger")) return .trigger;
        return null;
    }
};

pub const DesiredStatus = enum {
    active,
    paused,
    deleting,

    pub fn toSlice(self: DesiredStatus) []const u8 {
        return @tagName(self);
    }

    pub fn fromSlice(value: []const u8) ?DesiredStatus {
        inline for (std.meta.tags(DesiredStatus)) |tag| {
            if (std.mem.eql(u8, value, @tagName(tag))) return tag;
        }
        return null;
    }
};

pub const SyncStatus = enum {
    syncing,
    synced,
    failed,

    pub fn toSlice(self: SyncStatus) []const u8 {
        return @tagName(self);
    }

    pub fn fromSlice(value: []const u8) ?SyncStatus {
        inline for (std.meta.tags(SyncStatus)) |tag| {
            if (std.mem.eql(u8, value, @tagName(tag))) return tag;
        }
        return null;
    }
};

/// Fully owned schedule row. Every slice is allocated by the caller-provided
/// allocator used by the store; `deinit` releases all of them.
pub const Schedule = struct {
    schedule_id: []const u8,
    fleet_id: []const u8,
    source: Source,
    source_key: []const u8,
    cron: []const u8,
    timezone: []const u8,
    message: []const u8,
    desired_status: DesiredStatus,
    sync_status: SyncStatus,
    generation: i64,
    sync_token: ?[]const u8,
    sync_lease_until: ?i64,
    last_error: ?[]const u8,
    created_at: i64,
    updated_at: i64,

    pub fn deinit(self: *Schedule, alloc: std.mem.Allocator) void {
        alloc.free(self.schedule_id);
        alloc.free(self.fleet_id);
        alloc.free(self.source_key);
        alloc.free(self.cron);
        alloc.free(self.timezone);
        alloc.free(self.message);
        if (self.sync_token) |value| alloc.free(value);
        if (self.last_error) |value| alloc.free(value);
        self.* = undefined;
    }
};

pub const CreateInput = struct {
    fleet_id: []const u8,
    source: Source,
    source_key: []const u8,
    cron: []const u8,
    timezone: []const u8 = DEFAULT_TIMEZONE,
    message: []const u8,
};

pub const MutationInput = struct {
    schedule_id: []const u8,
    fleet_id: []const u8,
    cron: []const u8,
    timezone: []const u8,
    message: []const u8,
    desired_status: DesiredStatus,
    lease_token: []const u8,
    now_ms: i64,
    lease_until_ms: i64,
};
