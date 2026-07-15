const std = @import("std");
const common = @import("common");

const QStashClient = @import("QStashClient.zig");
const model = @import("model.zig");

const SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-105000000201";
const DESTINATION = "https://api.agentsfleet.net/v1/ingress/qstash/schedules";
const TOKEN = "qstash-test-token";
const STALL_DEADLINE_MS: u31 = 250;
const STALL_ELAPSED_BOUND_MS: i64 = 2_000;

const Fake = struct {
    status: u16 = 200,
    response_body: []const u8 = "{\"scheduleId\":\"0195b4ba-8d3a-7f13-8abc-105000000201\"}",
    failure: ?anyerror = null,
    calls: usize = 0,
    request_ok: bool = false,

    fn exchange(self: *Fake) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *Fake = @ptrCast(@alignCast(ptr));
        self.calls += 1;
        if (self.failure) |failure| return failure;
        self.request_ok = requestMatches(request);
        return .{ .status = self.status, .body = try alloc.dupe(u8, self.response_body) };
    }

    fn requestMatches(request: QStashClient.Request) bool {
        if (request.method == .POST) {
            if (!std.mem.eql(u8, request.url, "https://qstash.test/v2/schedules/https%3A%2F%2Fapi.agentsfleet.net%2Fv1%2Fingress%2Fqstash%2Fschedules")) return false;
            if (!headerEquals(request.headers, "authorization", "Bearer " ++ TOKEN)) return false;
            if (!headerEquals(request.headers, "Upstash-Cron", "CRON_TZ=Asia/Kolkata 0 9 * * *")) return false;
            if (!headerEquals(request.headers, "Upstash-Schedule-Id", SCHEDULE_ID)) return false;
            if (!headerEquals(request.headers, "Upstash-Method", "POST")) return false;
            if (!headerEquals(request.headers, "Upstash-Retries", "3")) return false;
            return std.mem.eql(u8, request.body, "{\"schedule_id\":\"" ++ SCHEDULE_ID ++ "\",\"generation\":7}");
        }
        return request.method == .DELETE and
            std.mem.eql(u8, request.url, "https://qstash.test/v2/schedules/" ++ SCHEDULE_ID) and
            headerEquals(request.headers, "authorization", "Bearer " ++ TOKEN) and
            request.body.len == 0;
    }
};

fn headerEquals(headers: []const std.http.Header, name: []const u8, expected: []const u8) bool {
    for (headers) |header| {
        if (std.ascii.eqlIgnoreCase(header.name, name)) return std.mem.eql(u8, header.value, expected);
    }
    return false;
}

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname initializes address before its port is read on success.
    var address: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&address), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, address.port);
}

fn schedule() model.Schedule {
    return .{
        .schedule_id = SCHEDULE_ID,
        .fleet_id = "0195b4ba-8d3a-7f13-8abc-105000000202",
        .source = .api,
        .source_key = "api:test",
        .cron = "0 9 * * *",
        .timezone = "Asia/Kolkata",
        .message = "summarize",
        .desired_status = .active,
        .sync_status = .syncing,
        .generation = 7,
        .sync_token = null,
        .sync_lease_until = null,
        .last_error = null,
        .created_at = 0,
        .updated_at = 0,
    };
}

test "qstash client: upsert sends the exact stable schedule request" {
    var fake: Fake = .{};
    const client = QStashClient.initWithBase(fake.exchange(), "https://qstash.test", DESTINATION);
    try std.testing.expectEqual(.success, try client.upsert(std.testing.allocator, TOKEN, schedule()));
    try std.testing.expectEqual(@as(usize, 1), fake.calls);
    try std.testing.expect(fake.request_ok);
}

test "qstash client: delete is idempotent when the provider row is absent" {
    var fake: Fake = .{ .status = 404, .response_body = "" };
    const client = QStashClient.initWithBase(fake.exchange(), "https://qstash.test", DESTINATION);
    try std.testing.expectEqual(.success, try client.delete(std.testing.allocator, TOKEN, SCHEDULE_ID));
    try std.testing.expectEqual(@as(usize, 1), fake.calls);
    try std.testing.expect(fake.request_ok);
}

test "qstash client: provider status and reply failures stay typed" {
    const cases = [_]struct { status: u16, body: []const u8, expected: QStashClient.Outcome }{
        .{ .status = 400, .body = "{}", .expected = .invalid_request },
        .{ .status = 412, .body = "{}", .expected = .invalid_request },
        .{ .status = 429, .body = "{}", .expected = .rate_limited },
        .{ .status = 503, .body = "{}", .expected = .unavailable },
        .{ .status = 200, .body = "not-json", .expected = .malformed_response },
        .{ .status = 200, .body = "{\"scheduleId\":\"wrong\"}", .expected = .malformed_response },
    };
    for (cases) |case| {
        var fake: Fake = .{ .status = case.status, .response_body = case.body };
        const client = QStashClient.initWithBase(fake.exchange(), "https://qstash.test", DESTINATION);
        try std.testing.expectEqual(case.expected, try client.upsert(std.testing.allocator, TOKEN, schedule()));
        try std.testing.expectEqual(@as(usize, 1), fake.calls);
    }
}

test "qstash client: transport uncertainty makes one attempt and returns unavailable" {
    var fake: Fake = .{ .failure = error.ResponseLost };
    const client = QStashClient.initWithBase(fake.exchange(), "https://qstash.test", DESTINATION);
    try std.testing.expectEqual(.unavailable, try client.upsert(std.testing.allocator, TOKEN, schedule()));
    try std.testing.expectEqual(@as(usize, 1), fake.calls);
}

test "qstash client: production transport refuses an unusable URL" {
    var exchange: QStashClient.HttpClientExchange = .{ .io = common.globalIo() };
    const client = QStashClient.initWithBase(exchange.exchange(), "not a url", DESTINATION);
    try std.testing.expectEqual(.unavailable, try client.delete(std.testing.allocator, TOKEN, SCHEDULE_ID));
}

test "qstash client: production transport deadline cuts off a stalled provider" {
    const io = common.globalIo();
    var address = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var base_buffer: [48]u8 = undefined;
    const base = try std.fmt.bufPrint(&base_buffer, "http://127.0.0.1:{d}", .{port});
    var exchange: QStashClient.HttpClientExchange = .{ .io = io, .deadline_ms = STALL_DEADLINE_MS };
    const client = QStashClient.initWithBase(exchange.exchange(), base, DESTINATION);

    const started_at = common.clock.nowMillis();
    const outcome = try client.delete(std.testing.allocator, TOKEN, SCHEDULE_ID);
    const elapsed_ms = common.clock.nowMillis() - started_at;
    try std.testing.expectEqual(.unavailable, outcome);
    try std.testing.expect(elapsed_ms < STALL_ELAPSED_BOUND_MS);
}

fn allocationSweep(alloc: std.mem.Allocator) !void {
    var fake: Fake = .{};
    const client = QStashClient.initWithBase(fake.exchange(), "https://qstash.test", DESTINATION);
    const outcome = try client.upsert(alloc, TOKEN, schedule());
    try std.testing.expectEqual(.success, outcome);
}

test "qstash client: every allocation failure unwinds without a leak" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationSweep, .{});
}
