//! POST /v1/ingress/{provider} — bearer-less App-webhook ingress.
//! Provider verification, routing paths, headers, normalization, and replay
//! namespace come from `webhook_verify.PROVIDER_REGISTRY`; this handler has no
//! provider branch. The platform secret is read from the admin workspace vault.
//! Signature verification precedes JSON parsing and every routing side effect.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const hs = @import("hmac_sig");
const common_lib = @import("common");
const clock = common_lib.clock;

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const vault = @import("../../../state/vault.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const webhook_verify = @import("../../../fleet_runtime/webhook_verify.zig");
const grant_lookup = @import("../../../state/integration_grant_lookup.zig");
const EventEnvelope = @import("contract").event_envelope;
const sql = @import("ingress_sql.zig");

const log = logging.scoped(.app_ingress);
const Hx = hx_mod.Hx;

const MAX_BODY_SIZE: usize = 1024 * 1024;
const MAX_EVENT_LEN: usize = 64;
const MAX_DELIVERY_LEN: usize = 128;
const MAX_ROUTING_KEY_LEN: usize = 64;
const MAX_REPOSITORY_LEN: usize = 255;
const MAX_FANOUT: usize = 100;
const DEDUP_KEY_BUF_LEN: usize = 512;
const DEDUP_TTL_SECONDS: u32 = 72 * 60 * 60;
const LOG_INGRESS_REJECTED = "ingress_rejected";
const LOG_INGRESS_SECRET_LOAD_FAILED = "ingress_secret_load_failed";
const EVENT_PING = "ping";
const STATUS_PONG = "pong";
const S_PROVIDER_SECRET_MISSING = "Provider App webhook secret is not configured";
const S_PROVIDER_SECRET_LOAD_FAILED = "Failed to load provider App webhook secret";

const Target = struct {
    fleet_id: []const u8,
    workspace_id: []const u8,
};

pub fn innerIngress(hx: Hx, req: *httpz.Request, provider: []const u8) void {
    const verify = webhook_verify.detectProvider(provider, webhook_verify.NoHeaders{}) orelse return unknownProvider(hx);
    const ingress = verify.ingress orelse return unknownProvider(hx);
    const event = boundedHeader(req, ingress.event_header, MAX_EVENT_LEN) orelse return malformed(hx);
    const delivery = boundedHeader(req, ingress.delivery_header, MAX_DELIVERY_LEN) orelse return malformed(hx);
    const signature = req.header(verify.sig_header) orelse return invalidSignature(hx, provider);
    const body = req.body() orelse "";
    if (body.len > MAX_BODY_SIZE) return hx.fail(ec.ERR_WEBHOOK_PAYLOAD_TOO_LARGE, "Webhook body exceeds the 1 MiB ingress limit");

    var conn_slot: ?*pg.Conn = hx.ctx.pool.acquire() catch return common.internalDbUnavailable(hx.res, hx.req_id);
    defer if (conn_slot) |conn| hx.ctx.pool.release(conn);
    const secret = loadPlatformSecret(hx.alloc, conn_slot.?, hx.ctx.platform_admin_workspace_id, ingress) catch |err| {
        log.err(LOG_INGRESS_SECRET_LOAD_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = provider, .err = @errorName(err) });
        common.internalOperationError(hx.res, S_PROVIDER_SECRET_LOAD_FAILED, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_PROVIDER_SECRET_MISSING);
        return;
    };
    defer hx.alloc.free(secret);
    if (!validSignature(verify, secret, signature, body)) return invalidSignature(hx, provider);

    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch return malformed(hx);
    defer parsed.deinit();
    const root = switch (parsed.value) {
        .object => |obj| obj,
        else => return malformed(hx),
    };
    if (std.mem.eql(u8, event, EVENT_PING)) {
        hx.ok(.ok, .{ .status = STATUS_PONG });
        return;
    }
    const replay_id = authenticatedReplayId(body);
    const routing_key = extractOwnedScalar(hx.alloc, root, ingress.routing_key_path, MAX_ROUTING_KEY_LEN) catch return malformed(hx);
    defer hx.alloc.free(routing_key);
    const repository = extractString(root, ingress.repository_path, MAX_REPOSITORY_LEN) orelse return malformed(hx);
    const normalized = ingress.normalize(hx.alloc, event, root, clock.nowSeconds()) catch {
        malformed(hx);
        return;
    };
    const request_json = normalized orelse {
        hx.ok(.ok, .{ .status = "ignored" });
        return;
    };
    defer hx.alloc.free(request_json);

    routeNormalized(hx, &conn_slot, ingress, provider, event, delivery, &replay_id, routing_key, repository, request_json);
}

fn routeNormalized(hx: Hx, conn_slot: *?*pg.Conn, ingress: webhook_verify.IngressConfig, provider: []const u8, event: []const u8, delivery: []const u8, replay_id: []const u8, routing_key: []const u8, repository: []const u8, request_json: []const u8) void {
    const resolved_workspace = resolveWorkspace(hx.alloc, conn_slot.*.?, provider, routing_key) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    const workspace_id = resolved_workspace orelse {
        log.warn(LOG_INGRESS_REJECTED, .{ .error_code = ec.ERR_WEBHOOK_INSTALL_NOT_MAPPED, .provider = provider, .reason = "unmapped_installation", .delivery = delivery });
        hx.ok(.ok, .{ .ignored = ec.ERR_WEBHOOK_INSTALL_NOT_MAPPED });
        return;
    };
    defer hx.alloc.free(workspace_id);
    const targets = findTargets(hx.alloc, conn_slot.*.?, workspace_id, provider, repository, event) catch |err| {
        log.err("ingress_target_lookup_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .provider = provider, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer freeTargets(hx.alloc, targets);
    hx.ctx.pool.release(conn_slot.*.?);
    conn_slot.* = null;

    if (targets.len == 0) {
        log.info("ingress_ignored", .{ .error_code = ec.ERR_WEBHOOK_SUBSCRIPTION_NOT_FOUND, .provider = provider, .reason = "no_matching_subscription", .delivery = delivery });
        hx.ok(.ok, .{ .ignored = ec.ERR_WEBHOOK_SUBSCRIPTION_NOT_FOUND });
        return;
    }
    fanOut(hx, ingress, provider, delivery, replay_id, request_json, targets);
}

fn fanOut(hx: Hx, ingress: webhook_verify.IngressConfig, provider: []const u8, delivery: []const u8, replay_id: []const u8, request_json: []const u8, targets: []const Target) void {
    var enqueued: usize = 0;
    var failed = false;
    for (targets) |target| {
        var key_buf: [DEDUP_KEY_BUF_LEN]u8 = undefined;
        const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}:{s}", .{ ec.WEBHOOK_DEDUP_KEY_PREFIX, target.fleet_id, ingress.dedup_namespace, replay_id }) catch {
            failed = true;
            continue;
        };
        const is_new = hx.ctx.queue.setNx(key, "1", DEDUP_TTL_SECONDS) catch {
            failed = true;
            continue;
        };
        if (!is_new) continue;
        const envelope = EventEnvelope{ .event_id = "", .fleet_id = target.fleet_id, .workspace_id = target.workspace_id, .actor = ingress.actor, .event_type = .webhook, .request_json = request_json, .created_at = clock.nowMillis() };
        const event_id = hx.ctx.queue.xaddFleetEvent(envelope) catch {
            releaseSlot(hx, target.fleet_id, key);
            failed = true;
            continue;
        };
        hx.ctx.alloc.free(event_id);
        enqueued += 1;
    }
    if (failed) {
        log.err("ingress_fanout_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = provider, .delivery = delivery, .enqueued = enqueued });
        common.internalOperationError(hx.res, "Failed to enqueue every matching fleet event", hx.req_id);
        return;
    }
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .matched = targets.len, .enqueued = enqueued });
}

fn authenticatedReplayId(body: []const u8) [std.crypto.hash.sha2.Sha256.digest_length * 2]u8 {
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(body, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

fn releaseSlot(hx: Hx, fleet_id: []const u8, key: []const u8) void {
    hx.ctx.queue.del(key) catch |err| log.warn("ingress_dedup_release_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
}

fn validSignature(config: webhook_verify.VerifyConfig, secret: []const u8, provided: []const u8, body: []const u8) bool {
    if (secret.len == 0 or !std.mem.startsWith(u8, provided, config.prefix)) return false;
    const expected = hs.hexDecode32(provided[config.prefix.len..]) orelse return false;
    const actual = hs.computeMac(secret, &.{body});
    return hs.constantTimeEql(&actual, &expected);
}

fn loadPlatformSecret(alloc: std.mem.Allocator, conn: *pg.Conn, admin_workspace_id: []const u8, ingress: webhook_verify.IngressConfig) !?[]u8 {
    if (admin_workspace_id.len == 0) return null;
    var parsed = vault.loadJson(alloc, conn, admin_workspace_id, ingress.platform_secret_key) catch |err| switch (err) {
        error.NotFound => return null,
        else => return err,
    };
    defer parsed.deinit();
    const value = parsed.value.object.get(ingress.platform_secret_field) orelse return null;
    if (value != .string or value.string.len == 0) return null;
    return try alloc.dupe(u8, value.string);
}

fn resolveWorkspace(alloc: std.mem.Allocator, conn: *pg.Conn, provider: []const u8, routing_key: []const u8) !?[]u8 {
    var q = PgQuery.from(try conn.query(sql.SELECT_WORKSPACE, .{ provider, routing_key }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

fn findTargets(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, provider: []const u8, repository: []const u8, event: []const u8) ![]const Target {
    var out: std.ArrayList(Target) = .empty;
    errdefer freeTargets(alloc, out.items);
    var q = PgQuery.from(try conn.query(sql.SELECT_TARGETS, .{ workspace_id, fleet_config.FleetStatus.active.toSlice(), provider, grant_lookup.GrantStatus.approved.toSlice(), repository, event, MAX_FANOUT + 1 }));
    defer q.deinit();
    while (try q.next()) |row| {
        if (out.items.len == MAX_FANOUT) return error.FanoutLimitExceeded;
        const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(fleet_id);
        const target_workspace_id = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(target_workspace_id);
        try out.append(alloc, .{ .fleet_id = fleet_id, .workspace_id = target_workspace_id });
    }
    return out.toOwnedSlice(alloc);
}

fn freeTargets(alloc: std.mem.Allocator, targets: []const Target) void {
    for (targets) |target| {
        alloc.free(target.fleet_id);
        alloc.free(target.workspace_id);
    }
    alloc.free(targets);
}

fn extractOwnedScalar(alloc: std.mem.Allocator, root: std.json.ObjectMap, path: []const []const u8, max_len: usize) ![]u8 {
    const value = extractValue(root, path) orelse return error.MissingRoutingKey;
    const out = switch (value) {
        .string => |s| try alloc.dupe(u8, s),
        .integer => |i| try std.fmt.allocPrint(alloc, "{d}", .{i}),
        else => return error.InvalidRoutingKey,
    };
    errdefer alloc.free(out);
    if (out.len == 0 or out.len > max_len) return error.InvalidRoutingKey;
    return out;
}

fn extractString(root: std.json.ObjectMap, path: []const []const u8, max_len: usize) ?[]const u8 {
    const value = extractValue(root, path) orelse return null;
    if (value != .string or value.string.len == 0 or value.string.len > max_len) return null;
    return value.string;
}

fn extractValue(root: std.json.ObjectMap, path: []const []const u8) ?std.json.Value {
    var object = root;
    for (path, 0..) |key, index| {
        const value = object.get(key) orelse return null;
        if (index + 1 == path.len) return value;
        object = switch (value) {
            .object => |nested| nested,
            else => return null,
        };
    }
    return null;
}

fn boundedHeader(req: *httpz.Request, name: []const u8, max_len: usize) ?[]const u8 {
    const value = req.header(name) orelse return null;
    return if (value.len > 0 and value.len <= max_len) value else null;
}

fn invalidSignature(hx: Hx, provider: []const u8) void {
    log.warn(LOG_INGRESS_REJECTED, .{ .error_code = ec.ERR_WEBHOOK_SIG_INVALID, .provider = provider, .reason = "bad_signature" });
    hx.fail(ec.ERR_WEBHOOK_SIG_INVALID, "Invalid webhook signature");
}

fn malformed(hx: Hx) void {
    hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
}

fn unknownProvider(hx: Hx) void {
    hx.fail(ec.ERR_CONNECTOR_UNKNOWN, "Unknown App ingress provider");
}

fn fakeNormalize(alloc: std.mem.Allocator, event: []const u8, _: std.json.ObjectMap, _: i64) anyerror!?[]u8 {
    return @as(?[]u8, try std.fmt.allocPrint(alloc, "{{\"event\":\"{s}\"}}", .{event}));
}

test "App ingress behavior is driven by a provider descriptor" {
    const descriptor = webhook_verify.IngressConfig{
        .platform_secret_key = "fake-app",
        .platform_secret_field = "secret",
        .routing_key_path = &.{ "account", "id" },
        .repository_path = &.{ "project", "name" },
        .event_header = "x-fake-event",
        .delivery_header = "x-fake-delivery",
        .dedup_namespace = "fake",
        .actor = "fake-app",
        .normalize = fakeNormalize,
    };
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"account\":{\"id\":42},\"project\":{\"name\":\"owner/repo\"}}", .{});
    defer parsed.deinit();
    const key = try extractOwnedScalar(std.testing.allocator, parsed.value.object, descriptor.routing_key_path, MAX_ROUTING_KEY_LEN);
    defer std.testing.allocator.free(key);
    try std.testing.expectEqualStrings("42", key);
    try std.testing.expectEqualStrings("owner/repo", extractString(parsed.value.object, descriptor.repository_path, MAX_REPOSITORY_LEN).?);
    const normalized = (try descriptor.normalize(std.testing.allocator, "changed", parsed.value.object, 0)).?;
    defer std.testing.allocator.free(normalized);
    try std.testing.expectEqualStrings("{\"event\":\"changed\"}", normalized);
}

test "App ingress replay identity is independent of the unsigned delivery header" {
    const first = authenticatedReplayId("signed-body");
    const replay = authenticatedReplayId("signed-body");
    const different = authenticatedReplayId("different-signed-body");
    try std.testing.expectEqualSlices(u8, &first, &replay);
    try std.testing.expect(!std.mem.eql(u8, &first, &different));
}

test "App ingress replay slot covers the GitHub redelivery window" {
    try std.testing.expectEqual(@as(u32, 72 * 60 * 60), DEDUP_TTL_SECONDS);
}
