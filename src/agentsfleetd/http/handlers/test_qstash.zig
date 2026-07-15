//! Shared fake QStash wiring for HTTP handler integration tests.

const std = @import("std");

const Credentials = @import("../../cron/Credentials.zig");
const QStashClient = @import("../../cron/QStashClient.zig");
const harness_mod = @import("../test_harness.zig");

const TOKEN_VALUE = "test-qstash-token";
const CURRENT_SIGNING_KEY_VALUE = "test-qstash-current";
const NEXT_SIGNING_KEY_VALUE = "test-qstash-next";
const EMPTY_JSON = "{}";
const EMPTY_BODY = "";

var token_bytes = TOKEN_VALUE.*;
var current_signing_key_bytes = CURRENT_SIGNING_KEY_VALUE.*;
var next_signing_key_bytes = NEXT_SIGNING_KEY_VALUE.*;
var default_credentials: Credentials = .{
    .token = token_bytes[0..TOKEN_VALUE.len],
    .current_signing_key = current_signing_key_bytes[0..CURRENT_SIGNING_KEY_VALUE.len],
    .next_signing_key = next_signing_key_bytes[0..NEXT_SIGNING_KEY_VALUE.len],
};
var default_fake: FakeQStash = .{};

pub const FakeQStash = struct {
    status: std.atomic.Value(u16) = .init(200),
    calls: std.atomic.Value(u32) = .init(0),

    pub fn exchange(self: *FakeQStash) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *FakeQStash = @ptrCast(@alignCast(ptr));
        // safe because: tests only need an eventually-correct call counter.
        _ = self.calls.fetchAdd(1, .monotonic);
        // safe because: status may be changed by the test thread before calls.
        const status = self.status.load(.acquire);
        if (status != 200 and status != 204) return .{ .status = status, .body = try alloc.dupe(u8, EMPTY_JSON) };
        if (request.method == .DELETE) return .{ .status = 204, .body = try alloc.dupe(u8, EMPTY_BODY) };
        const Parsed = struct { schedule_id: []const u8 };
        var parsed = try std.json.parseFromSlice(Parsed, alloc, request.body, .{ .ignore_unknown_fields = true });
        defer parsed.deinit();
        return .{
            .status = 200,
            .body = try std.fmt.allocPrint(alloc, "{{\"scheduleId\":\"{s}\"}}", .{parsed.value.schedule_id}),
        };
    }
};

pub fn attachSuccess(harness: *harness_mod.TestHarness) void {
    harness.ctx.qstash_credentials = &default_credentials;
    harness.ctx.qstash_exchange_override = default_fake.exchange();
}
