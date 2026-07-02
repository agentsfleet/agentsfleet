//! Generic API-key connector archetype. The registry entry declares the fields
//! a provider needs and the cheapest validation probe; this handler validates
//! the submitted JSON, runs one deadline-armed probe, then writes the
//! `fleet:<provider>` vault handle only after the probe succeeds.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const bounded_fetch = @import("bounded_fetch.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");

const FIELD_INTEGRATION = "integration";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON";
const S_PROBE_REJECTED = "Connector probe rejected the supplied credentials";
const S_PROBE_DEADLINE = "Connector validation probe did not complete in time";
const S_CONNECT_FAILED = "Failed to store connector credentials";
const HEADER_ACCEPT = "accept";
const HEADER_AUTHORIZATION = "authorization";
const CONTENT_TYPE_JSON = "application/json";

/// One JSON field accepted by an API-key connector. Secret fields are stored in
/// the vault handle and never echoed in responses or logs.
pub const Field = struct {
    name: []const u8,
    secret: bool = true,
};

/// Cheapest authenticated endpoint for validating a submitted key.
pub const Probe = struct {
    base_url: []const u8,
    path: []const u8,
    auth: Auth,

    pub const Auth = union(enum) {
        bearer_field: []const u8,
        datadog_keys: struct { api_key_field: []const u8, app_key_field: []const u8 },
    };
};

/// Registry data for the API-key archetype.
pub const Data = struct {
    fields: []const Field,
    probe: Probe,
};

/// API-key connect arm: parse declared fields, probe the vendor, then persist
/// `fleet:<provider>` as a JSON vault handle. No state signing secret is needed
/// because there is no browser callback.
pub fn connect(hx: hx_mod.Hx, req: *httpz.Request, conn: *pg.Conn, provider: []const u8, data: Data, workspace_id: []const u8) void {
    const body = req.body() orelse return hx.fail(ec.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch
        return hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_JSON),
    };
    if (!hasRequiredFields(obj, data.fields)) return hx.fail(ec.ERR_INVALID_REQUEST, "Missing required connector credential field");

    runProbe(hx, provider, data.probe, obj) catch |err| switch (err) {
        error.ProbeRejected => return hx.fail(ec.ERR_CONNECTOR_PROBE_REJECTED, S_PROBE_REJECTED),
        error.DeadlineExceeded, error.WatchdogUnavailable, error.VendorUnreachable => return hx.fail(ec.ERR_CONNECTOR_VENDOR_DEADLINE, S_PROBE_DEADLINE),
        else => return common.internalOperationError(hx.res, "Failed to validate connector credentials", hx.req_id),
    };

    storeHandle(hx, conn, workspace_id, provider, data.fields, obj) catch {
        common.internalOperationError(hx.res, S_CONNECT_FAILED, hx.req_id);
        return;
    };
    hx.ok(.ok, .{ .status = STATUS_CONNECTED });
}

/// Shared status hook for API-key connectors. A handle with the integration field
/// reads connected; missing/unreadable reads not_connected.
pub fn respondStatus(hx: hx_mod.Hx, handle: ?std.json.ObjectMap) void {
    const connected = if (handle) |obj| obj.get(FIELD_INTEGRATION) != null else false;
    hx.ok(.ok, .{ .status = if (connected) STATUS_CONNECTED else STATUS_NOT_CONNECTED });
}

fn hasRequiredFields(obj: std.json.ObjectMap, fields: []const Field) bool {
    for (fields) |field| {
        const v = obj.get(field.name) orelse return false;
        if (v != .string or v.string.len == 0) return false;
    }
    return true;
}

fn storeHandle(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8, provider: []const u8, fields: []const Field, body: std.json.ObjectMap) !void {
    var handle: std.json.ObjectMap = .empty;
    defer handle.deinit(hx.alloc);
    try handle.put(hx.alloc, FIELD_INTEGRATION, .{ .string = provider });
    for (fields) |field| try handle.put(hx.alloc, field.name, body.get(field.name).?);

    const key = try credential_key.allocKeyName(hx.alloc, provider);
    defer hx.alloc.free(key);
    const json = try std.json.Stringify.valueAlloc(hx.alloc, std.json.Value{ .object = handle }, .{});
    defer hx.alloc.free(json);
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, key, json);
}

fn runProbe(hx: hx_mod.Hx, provider: []const u8, probe: Probe, fields: std.json.ObjectMap) !void {
    const base = hx.ctx.connector_api_key_probe_base_override orelse probe.base_url;
    const url = try std.fmt.allocPrint(hx.alloc, "{s}{s}", .{ base, probe.path });
    defer hx.alloc.free(url);

    var headers_buf: [3]std.http.Header = undefined;
    const headers = try probeHeaders(hx.alloc, probe.auth, fields, &headers_buf);
    defer freeHeaders(hx.alloc, headers);

    var wd: bounded_fetch.Watchdog = .{};
    defer wd.deinit();
    const resp = try bounded_fetch.fetch(hx.alloc, hx.ctx.io, &wd, .{
        .url = url,
        .method = .GET,
        .extra_headers = headers,
        .deadline_ms = hx.ctx.connector_api_key_probe_deadline_ms_override orelse bounded_fetch.TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = provider,
        .class = .token_exchange,
    });
    defer hx.alloc.free(resp.body);
    if (resp.status < 200 or resp.status >= 300) return error.ProbeRejected;
}

fn probeHeaders(alloc: std.mem.Allocator, auth: Probe.Auth, fields: std.json.ObjectMap, buf: *[3]std.http.Header) ![]const std.http.Header {
    switch (auth) {
        .bearer_field => |name| {
            buf[0] = .{ .name = HEADER_AUTHORIZATION, .value = try std.fmt.allocPrint(alloc, "Bearer {s}", .{strField(fields, name).?}) };
            buf[1] = .{ .name = HEADER_ACCEPT, .value = CONTENT_TYPE_JSON };
            return buf[0..2];
        },
        .datadog_keys => |d| {
            buf[0] = .{ .name = "dd-api-key", .value = strField(fields, d.api_key_field).? };
            buf[1] = .{ .name = "dd-application-key", .value = strField(fields, d.app_key_field).? };
            buf[2] = .{ .name = HEADER_ACCEPT, .value = CONTENT_TYPE_JSON };
            return buf[0..3];
        },
    }
}

fn freeHeaders(alloc: std.mem.Allocator, headers: []const std.http.Header) void {
    for (headers) |h| {
        if (std.ascii.eqlIgnoreCase(h.name, HEADER_AUTHORIZATION)) alloc.free(h.value);
    }
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

// ── Tests (request/DB path is integration-tested through connect.zig) ───────

const testing = std.testing;

test "api_key: required fields must all be non-empty strings" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"api_key\":\"k\",\"site\":\"us\"}", .{});
    defer parsed.deinit();
    const fields = [_]Field{ .{ .name = "api_key" }, .{ .name = "site", .secret = false } };
    try testing.expect(hasRequiredFields(parsed.value.object, &fields));
    const missing = [_]Field{ .{ .name = "api_key" }, .{ .name = "app_key" } };
    try testing.expect(!hasRequiredFields(parsed.value.object, &missing));
}
