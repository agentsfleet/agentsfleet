//! Native Upstash QStash schedule client.
//!
//! One caller request performs one provider attempt. The stable agentsfleet
//! schedule identifier is also the QStash identifier, so explicit sync safely
//! overwrites uncertain state without a background retry loop.

const QStashClient = @This();

const std = @import("std");
const call_deadline = @import("call_deadline");
const http_pin = @import("http_pin");

const model = @import("model.zig");

const SCHEDULES_PATH = "/v2/schedules/";
const CONTENT_TYPE_JSON = "application/json";
const AUTHORIZATION_HEADER = "authorization";
const CONTENT_TYPE_HEADER = "content-type";
const CRON_HEADER = "Upstash-Cron";
const METHOD_HEADER = "Upstash-Method";
const RETRIES_HEADER = "Upstash-Retries";
const SCHEDULE_ID_HEADER = "Upstash-Schedule-Id";
const DELIVERY_METHOD = "POST";
const DELIVERY_RETRIES = "3";
const AUTHORIZATION_FORMAT = "Bearer {s}";
const ENDPOINT_FORMAT = "{s}{s}{s}";

const AUTHORIZATION_BUFFER_LEN: usize = 1024;
const CRON_HEADER_BUFFER_LEN: usize = 256;
const RESPONSE_HEAD_BUFFER_LEN: usize = 8 * 1024;
const RESPONSE_TRANSFER_BUFFER_LEN: usize = 4 * 1024;
const DRAIN_CHUNK_BYTES: usize = 1024;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024;
pub const DEADLINE_MS: u31 = 10_000;

const Scheduler = call_deadline.ProcessScheduler;

exchange: Exchange,
api_base: []const u8,
destination_url: []const u8,

pub const Request = struct {
    url: []const u8,
    method: std.http.Method,
    headers: []const std.http.Header,
    body: []const u8,
};

pub const Response = struct {
    status: u16,
    body: []u8,
};

pub const Exchange = struct {
    ptr: *anyopaque,
    callFn: *const fn (*anyopaque, std.mem.Allocator, Request) anyerror!Response,

    pub fn call(self: Exchange, alloc: std.mem.Allocator, request: Request) anyerror!Response {
        return self.callFn(self.ptr, alloc, request);
    }
};

pub const Outcome = enum {
    success,
    invalid_request,
    rate_limited,
    unavailable,
    malformed_response,
};

pub fn init(exchange: Exchange, api_base: []const u8, destination_url: []const u8) QStashClient {
    return .{ .exchange = exchange, .api_base = api_base, .destination_url = destination_url };
}

pub fn upsert(
    self: QStashClient,
    alloc: std.mem.Allocator,
    token: []const u8,
    schedule: model.Schedule,
) error{OutOfMemory}!Outcome {
    // QStash takes the destination raw in the path segment (it parses the
    // destination URL itself and forwards its query params). Percent-encoding it
    // makes QStash reject the create with "invalid destination url: endpoint has
    // invalid scheme" — proven by the live dev-server integration test.
    const url = try std.fmt.allocPrint(alloc, ENDPOINT_FORMAT, .{ self.api_base, SCHEDULES_PATH, self.destination_url });
    defer alloc.free(url);
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .schedule_id = schedule.schedule_id,
        .generation = schedule.generation,
    }, .{});
    defer alloc.free(body);

    var authorization_buffer: [AUTHORIZATION_BUFFER_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &authorization_buffer);
    const authorization = std.fmt.bufPrint(&authorization_buffer, AUTHORIZATION_FORMAT, .{token}) catch return .invalid_request;
    var cron_buffer: [CRON_HEADER_BUFFER_LEN]u8 = undefined;
    const cron = std.fmt.bufPrint(&cron_buffer, "CRON_TZ={s} {s}", .{ schedule.timezone, schedule.cron }) catch return .invalid_request;
    const headers = [_]std.http.Header{
        .{ .name = AUTHORIZATION_HEADER, .value = authorization },
        .{ .name = CONTENT_TYPE_HEADER, .value = CONTENT_TYPE_JSON },
        .{ .name = CRON_HEADER, .value = cron },
        .{ .name = SCHEDULE_ID_HEADER, .value = schedule.schedule_id },
        .{ .name = METHOD_HEADER, .value = DELIVERY_METHOD },
        .{ .name = RETRIES_HEADER, .value = DELIVERY_RETRIES },
    };
    const response = self.exchange.call(alloc, .{
        .url = url,
        .method = .POST,
        .headers = &headers,
        .body = body,
    }) catch |err| return exchangeFailure(err);
    defer alloc.free(response.body);
    return classifyUpsert(alloc, response, schedule.schedule_id);
}

pub fn delete(
    self: QStashClient,
    alloc: std.mem.Allocator,
    token: []const u8,
    schedule_id: []const u8,
) error{OutOfMemory}!Outcome {
    const encoded_id = try percentEncode(alloc, schedule_id);
    defer alloc.free(encoded_id);
    const url = try std.fmt.allocPrint(alloc, ENDPOINT_FORMAT, .{ self.api_base, SCHEDULES_PATH, encoded_id });
    defer alloc.free(url);
    var authorization_buffer: [AUTHORIZATION_BUFFER_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &authorization_buffer);
    const authorization = std.fmt.bufPrint(&authorization_buffer, AUTHORIZATION_FORMAT, .{token}) catch return .invalid_request;
    const headers = [_]std.http.Header{.{ .name = AUTHORIZATION_HEADER, .value = authorization }};
    const response = self.exchange.call(alloc, .{
        .url = url,
        .method = .DELETE,
        .headers = &headers,
        .body = &.{},
    }) catch |err| return exchangeFailure(err);
    defer alloc.free(response.body);
    if (response.status == 200 or response.status == 204 or response.status == 404) return .success;
    return classifyStatus(response.status);
}

fn classifyUpsert(alloc: std.mem.Allocator, response: Response, expected_id: []const u8) error{OutOfMemory}!Outcome {
    // QStash returns 201 Created for a schedule upsert (proven against the live
    // dev server); 200 is accepted too for forward-compatibility. Both are the
    // success path — the body still carries the echoed scheduleId.
    if (response.status != 200 and response.status != 201) return classifyStatus(response.status);
    const Reply = struct { scheduleId: []const u8 };
    var parsed = std.json.parseFromSlice(Reply, alloc, response.body, .{ .ignore_unknown_fields = true }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return .malformed_response,
    };
    defer parsed.deinit();
    if (!std.mem.eql(u8, parsed.value.scheduleId, expected_id)) return .malformed_response;
    return .success;
}

fn classifyStatus(status: u16) Outcome {
    if (status == 400 or status == 412 or status == 422) return .invalid_request;
    if (status == 429) return .rate_limited;
    if (status >= 500) return .unavailable;
    return .malformed_response;
}

fn exchangeFailure(err: anyerror) error{OutOfMemory}!Outcome {
    return switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => .unavailable,
    };
}

fn percentEncode(alloc: std.mem.Allocator, raw: []const u8) error{OutOfMemory}![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    const encoded_capacity = std.math.mul(usize, raw.len, 3) catch return error.OutOfMemory;
    try out.ensureTotalCapacityPrecise(alloc, encoded_capacity);
    for (raw) |char| {
        if (isUnreserved(char)) {
            out.appendAssumeCapacity(char);
        } else {
            const hex = "0123456789ABCDEF";
            out.appendAssumeCapacity('%');
            out.appendAssumeCapacity(hex[char >> 4]);
            out.appendAssumeCapacity(hex[char & 0x0f]);
        }
    }
    return out.toOwnedSlice(alloc);
}

fn isUnreserved(char: u8) bool {
    return std.ascii.isAlphanumeric(char) or char == '-' or char == '.' or char == '_' or char == '~';
}

pub const HttpClientExchange = struct {
    io: std.Io,
    /// Borrowed process scheduler — the exchange owns no deadline thread.
    sched: *Scheduler,
    deadline_ms: u31 = DEADLINE_MS,

    pub fn exchange(self: *HttpClientExchange) Exchange {
        return .{ .ptr = self, .callFn = callImpl };
    }

    fn callImpl(ptr: *anyopaque, alloc: std.mem.Allocator, request: Request) anyerror!Response {
        const self: *HttpClientExchange = @ptrCast(@alignCast(ptr));
        var client: std.http.Client = .{ .allocator = alloc, .io = self.io };
        defer client.deinit();
        var owner: call_deadline.SocketOwner = .{};
        const generation = owner.beginAttempt();
        const handle = http_pin.pinPooledHandle(&client, request.url) orelse return error.TransportFailure;
        _ = owner.attachSocket(generation, handle);
        var guard = self.sched.arm(owner.target(generation), self.deadline_ms) catch
            return error.SchedulerUnavailable;
        defer {
            owner.endAttempt();
            _ = guard.finish();
        }

        const uri = std.Uri.parse(request.url) catch return error.TransportFailure;
        var req = client.request(request.method, uri, .{
            .redirect_behavior = .unhandled,
            .extra_headers = request.headers,
        }) catch |err| return mapTransportError(err, &owner);
        defer req.deinit();
        if (request.body.len == 0)
            req.sendBodiless() catch |err| return mapTransportError(err, &owner)
        else
            req.sendBodyComplete(@constCast(request.body)) catch |err| return mapTransportError(err, &owner);

        var head_buffer: [RESPONSE_HEAD_BUFFER_LEN]u8 = undefined;
        var response = req.receiveHead(&head_buffer) catch |err| return mapTransportError(err, &owner);
        var transfer_buffer: [RESPONSE_TRANSFER_BUFFER_LEN]u8 = undefined;
        const body = try drainCapped(alloc, response.reader(&transfer_buffer));
        return .{ .status = @intFromEnum(response.head.status), .body = body };
    }
};

fn mapTransportError(err: anyerror, owner: *call_deadline.SocketOwner) anyerror {
    if (err == error.OutOfMemory) return error.OutOfMemory;
    return if (owner.wasInterrupted()) error.DeadlineExceeded else error.TransportFailure;
}

fn drainCapped(alloc: std.mem.Allocator, reader: *std.Io.Reader) anyerror![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.ensureTotalCapacityPrecise(alloc, MAX_RESPONSE_BYTES + 1);
    var chunk: [DRAIN_CHUNK_BYTES]u8 = undefined;
    while (true) {
        const count = reader.readSliceShort(&chunk) catch return error.TransportFailure;
        if (count == 0) break;
        if (out.items.len + count > MAX_RESPONSE_BYTES) return error.ResponseTooLarge;
        out.appendSliceAssumeCapacity(chunk[0..count]);
    }
    return out.toOwnedSlice(alloc);
}
