//! Authenticated cron-fire resolution and enqueue orchestration.

const FireService = @This();

const std = @import("std");
const common = @import("common");

const id_format = @import("../types/id_format.zig");
const FireQueue = @import("FireQueue.zig");
const FireStore = @import("FireStore.zig");
const QStashVerifier = @import("QStashVerifier.zig");

const MAX_BODY_BYTES: usize = 1024;

store: FireStore,
queue: FireQueue,
verifier: QStashVerifier,

pub const Request = struct {
    signature: []const u8,
    schedule_id_header: []const u8,
    message_id_header: []const u8,
    raw_body: []const u8,
};

pub const IgnoreReason = enum { schedule_missing, state_inactive, generation_stale };
pub const Outcome = union(enum) {
    enqueued,
    duplicate,
    ignored: IgnoreReason,
};

const Body = struct {
    schedule_id: []const u8,
    generation: i64,
};

pub fn init(store: FireStore, queue: FireQueue, verifier: QStashVerifier) FireService {
    return .{ .store = store, .queue = queue, .verifier = verifier };
}

pub fn process(self: FireService, alloc: std.mem.Allocator, request: Request) !Outcome {
    return self.processAt(alloc, request, common.clock.nowSeconds(), common.clock.nowMillis());
}

pub fn processAt(
    self: FireService,
    alloc: std.mem.Allocator,
    request: Request,
    now_seconds: i64,
    now_ms: i64,
) !Outcome {
    var verified = try self.verifier.verifyAt(alloc, request.signature, request.raw_body, now_seconds);
    defer verified.deinit(alloc);
    if (request.message_id_header.len == 0 or
        request.message_id_header.len > QStashVerifier.MAX_MESSAGE_ID_BYTES)
        return error.InvalidMessageId;
    if (request.raw_body.len == 0 or request.raw_body.len > MAX_BODY_BYTES) return error.InvalidFireBody;
    var parsed = std.json.parseFromSlice(Body, alloc, request.raw_body, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.InvalidFireBody,
    };
    defer parsed.deinit();
    const body = parsed.value;
    if (!id_format.isUuidV7(body.schedule_id) or body.generation <= 0) return error.InvalidFireBody;
    if (!std.mem.eql(u8, body.schedule_id, request.schedule_id_header)) return error.ScheduleIdMismatch;

    var target = (try self.store.resolve(alloc, body.schedule_id)) orelse
        return .{ .ignored = .schedule_missing };
    defer target.deinit(alloc);
    if (target.generation != body.generation) return .{ .ignored = .generation_stale };
    if (!target.isRunnable(body.generation)) return .{ .ignored = .state_inactive };
    const event_json = try std.json.Stringify.valueAlloc(alloc, .{
        .message = target.message,
        .schedule_id = body.schedule_id,
        .generation = body.generation,
        .fired_at = now_ms,
    }, .{});
    defer alloc.free(event_json);
    return switch (try self.queue.enqueue(
        target.fleet_id,
        target.workspace_id,
        body.schedule_id,
        verified.message_id,
        request.message_id_header,
        event_json,
        now_ms,
    )) {
        .enqueued => .enqueued,
        .duplicate => .duplicate,
    };
}
