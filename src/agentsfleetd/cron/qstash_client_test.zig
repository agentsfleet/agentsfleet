const std = @import("std");
const common = @import("common");
const call_deadline = @import("call_deadline");

const QStashClient = @import("QStashClient.zig");
const model = @import("model.zig");

const SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-105000000201";
const DESTINATION = "https://api.agentsfleet.net/v1/ingress/qstash/schedules";
const TOKEN = "qstash-test-token";
// Opt-in live QStash (the dev server container, or `npx @upstash/qstash-cli dev`).
// Both env vars must be set for the live test to run; otherwise it self-skips.
const LIVE_URL_ENV = "AGENTSFLEET_QSTASH_LIVE_URL";
const LIVE_TOKEN_ENV = "AGENTSFLEET_QSTASH_LIVE_TOKEN";
// A publicly resolvable destination the dev server accepts (it does real DNS on
// the destination at create time); the schedule is deleted right after.
const LIVE_DESTINATION = "https://example.com";
const STALL_DEADLINE_MS: u31 = 250;
const STALL_ELAPSED_BOUND_MS: i64 = 2_000;

const Fake = struct {
    status: u16 = 200,
    response_body: []const u8 = "{\"scheduleId\":\"0195b4ba-8d3a-7f13-8abc-105000000201\"}",
    failure: ?anyerror = null,
    calls: usize = 0,
    request_ok: bool = false,
    // Opt-in so the shared fake stays leak-free for tests that never free it;
    // only the api-base regression test captures + frees the outbound URL.
    capture_url: bool = false,
    captured_url: ?[]u8 = null,

    fn exchange(self: *Fake) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *Fake = @ptrCast(@alignCast(ptr));
        self.calls += 1;
        if (self.failure) |failure| return failure;
        if (self.capture_url) self.captured_url = try alloc.dupe(u8, request.url);
        self.request_ok = requestMatches(request);
        return .{ .status = self.status, .body = try alloc.dupe(u8, self.response_body) };
    }

    fn requestMatches(request: QStashClient.Request) bool {
        if (request.method == .POST) {
            if (!std.mem.eql(u8, request.url, "https://qstash.test/v2/schedules/" ++ DESTINATION)) return false;
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
    const client = QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
    try std.testing.expectEqual(.success, try client.upsert(std.testing.allocator, TOKEN, schedule()));
    try std.testing.expectEqual(@as(usize, 1), fake.calls);
    try std.testing.expect(fake.request_ok);
}

test "qstash client: outbound url uses the configured api base, not a hardcoded host" {
    // Regression: pins that credentials.url flows to the request URL. Fails if
    // anyone reintroduces a hardcoded provider host (the pre-M105 US default).
    const eu_base = "https://qstash-eu-central-1.upstash.io";
    var fake: Fake = .{ .capture_url = true };
    const client = QStashClient.init(fake.exchange(), eu_base, DESTINATION);
    try std.testing.expectEqual(.success, try client.upsert(std.testing.allocator, TOKEN, schedule()));
    defer if (fake.captured_url) |u| std.testing.allocator.free(u);
    try std.testing.expect(fake.captured_url != null);
    try std.testing.expect(std.mem.startsWith(u8, fake.captured_url.?, eu_base ++ "/v2/schedules/"));
}

test "qstash client: delete is idempotent when the provider row is absent" {
    var fake: Fake = .{ .status = 404, .response_body = "" };
    const client = QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
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
        const client = QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
        try std.testing.expectEqual(case.expected, try client.upsert(std.testing.allocator, TOKEN, schedule()));
        try std.testing.expectEqual(@as(usize, 1), fake.calls);
    }
}

test "qstash client: transport uncertainty makes one attempt and returns unavailable" {
    var fake: Fake = .{ .failure = error.ResponseLost };
    const client = QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
    try std.testing.expectEqual(.unavailable, try client.upsert(std.testing.allocator, TOKEN, schedule()));
    try std.testing.expectEqual(@as(usize, 1), fake.calls);
}

test "qstash client: production transport refuses an unusable URL" {
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(std.testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var exchange: QStashClient.HttpClientExchange = .{ .io = common.globalIo(), .sched = &sched };
    const client = QStashClient.init(exchange.exchange(), "not a url", DESTINATION);
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
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(std.testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var exchange: QStashClient.HttpClientExchange = .{ .io = io, .sched = &sched, .deadline_ms = STALL_DEADLINE_MS };
    const client = QStashClient.init(exchange.exchange(), base, DESTINATION);

    const started_at = common.clock.nowMillis();
    const outcome = try client.delete(std.testing.allocator, TOKEN, SCHEDULE_ID);
    const elapsed_ms = common.clock.nowMillis() - started_at;
    try std.testing.expectEqual(.unavailable, outcome);
    try std.testing.expect(elapsed_ms < STALL_ELAPSED_BOUND_MS);
}

fn allocationSweep(alloc: std.mem.Allocator) !void {
    var fake: Fake = .{};
    const client = QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
    const outcome = try client.upsert(alloc, TOKEN, schedule());
    try std.testing.expectEqual(.success, outcome);
}

test "qstash client: every allocation failure unwinds without a leak" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationSweep, .{});
}

test "qstash client (live): a real qstash dev server accepts the schedule lifecycle" {
    // Live end-to-end against a real QStash (the piece the fake exchange cannot
    // prove): the configured url + token actually authenticate, and both the
    // publish and the removal are accepted with the exact request shapes this
    // client builds. `make test-integration` starts the compose `qstash` service
    // and exports the two vars below; self-skips when they are unset.
    const base = common.env.testLiveValue(LIVE_URL_ENV) orelse return error.SkipZigTest;
    const token = common.env.testLiveValue(LIVE_TOKEN_ENV) orelse return error.SkipZigTest;
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(std.testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var exchange: QStashClient.HttpClientExchange = .{ .io = common.globalIo(), .sched = &sched };
    const client = QStashClient.init(exchange.exchange(), base, LIVE_DESTINATION);
    const created = try client.upsert(std.testing.allocator, token, schedule());
    try std.testing.expectEqual(QStashClient.Outcome.success, created);
    // Asserted, not swallowed: a broken delete request shape would otherwise
    // pass silently and only surface when a real schedule failed to unregister.
    const removed = try client.delete(std.testing.allocator, token, SCHEDULE_ID);
    try std.testing.expectEqual(QStashClient.Outcome.success, removed);
}
