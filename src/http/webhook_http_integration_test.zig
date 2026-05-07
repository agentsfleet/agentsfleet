// End-to-end HTTP integration tests for the github webhook ingest path.
//
// Uses the shared TestHarness with the production webhook_sig middleware
// wired to serve_webhook_lookup so a 202 proves the full path:
//   router → middleware → vault lookup → handler → redis dedup → 202.
//
// LIVE DB ONLY. Tests skip when DB is not reachable. The Redis-backed B-tier
// scenarios additionally call h.tryConnectRedis() and skip when REDIS_TLS_URL
// is unavailable. Run via `make test-integration` (sets up both).

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const webhook_sig = @import("../auth/middleware/webhook_sig.zig");
const svix_signature = @import("../auth/middleware/svix_signature.zig");
const serve_webhook_lookup = @import("../cmd/serve_webhook_lookup.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");

const harness_mod = @import("test_harness.zig");
const fx_mod = @import("webhook_test_fixtures.zig");
const signers = @import("webhook_test_signers.zig");

const TestHarness = harness_mod.TestHarness;

// ── Middleware wiring ─────────────────────────────────────────────────────

var wired_webhook_sig: webhook_sig.WebhookSig(*pg.Pool) = undefined;
var wired_svix: svix_signature.SvixSignature(*pg.Pool) = undefined;

fn wireWebhookMiddleware(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    wired_webhook_sig = .{
        .lookup_ctx = h.pool,
        .lookup_fn = serve_webhook_lookup.lookup,
    };
    wired_svix = .{
        .lookup_ctx = h.pool,
        .lookup_fn = serve_webhook_lookup.lookupSvix,
    };
    reg.setWebhookSig(wired_webhook_sig.middleware());
    reg.setSvixSig(wired_svix.middleware());
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    fx_mod.setTestEncryptionKey();
    return TestHarness.start(alloc, .{ .configureRegistry = wireWebhookMiddleware });
}

// ── Fixture helpers ───────────────────────────────────────────────────────

const SECRET = "topsecret-github-key";
const FAILURE_BODY =
    \\{"action":"completed","workflow_run":{"id":42,"head_sha":"abc","conclusion":"failure","head_branch":"main","html_url":"u","name":"w","run_attempt":1},"repository":{"full_name":"o/r"}}
;
const SUCCESS_BODY =
    \\{"action":"completed","workflow_run":{"id":42,"conclusion":"success","run_attempt":1},"repository":{"full_name":"o/r"}}
;
const IN_PROGRESS_BODY =
    \\{"action":"in_progress","workflow_run":{"id":42,"conclusion":null,"run_attempt":1},"repository":{"full_name":"o/r"}}
;

const Setup = struct {
    h: *TestHarness,
    fx: fx_mod.Fixture,
    url: []u8,

    fn init(alloc: std.mem.Allocator, status: []const u8) !Setup {
        const h = try startHarness(alloc);
        errdefer h.deinit();
        const fx: fx_mod.Fixture = .{
            .tenant_id = fx_mod.ID_TENANT_A,
            .workspace_id = fx_mod.ID_WS_A,
            .zombie_id = fx_mod.ID_ZOMBIE_A,
        };
        const trigger = try fx_mod.buildTriggerConfig(alloc, "github", null);
        defer alloc.free(trigger);
        const conn = try h.acquireConn();
        try fx_mod.insertZombie(conn, fx, trigger);
        try fx_mod.insertWebhookCredential(alloc, conn, fx.workspace_id, "github", SECRET);
        if (!std.mem.eql(u8, status, "active")) {
            _ = try conn.exec("UPDATE core.zombies SET status = $1 WHERE id = $2::uuid", .{ status, fx.zombie_id });
        }
        h.releaseConn(conn);
        const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}/github", .{fx.zombie_id});
        return .{ .h = h, .fx = fx, .url = url };
    }

    fn deinit(self: *Setup, alloc: std.mem.Allocator) void {
        const conn = self.h.acquireConn() catch null;
        if (conn) |c| {
            fx_mod.cleanup(c, self.fx) catch {};
            self.h.releaseConn(c);
        }
        alloc.free(self.url);
        self.h.deinit();
    }
};

fn skipOrErr(err: anyerror) anyerror {
    return switch (err) {
        error.SkipZigTest => error.SkipZigTest,
        else => err,
    };
}

fn postSigned(
    alloc: std.mem.Allocator,
    s: *Setup,
    event: []const u8,
    delivery: []const u8,
    body: []const u8,
) !harness_mod.Response {
    const sig = try signers.signGithub(alloc, SECRET, body);
    defer sig.deinit(alloc);
    const r1 = s.h.post(s.url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", event);
    const r4 = try r3.header("x-github-delivery", delivery);
    const r5 = try r4.json(body);
    return try r5.send();
}

// Issue a raw command and return the integer reply (or null on non-integer).
fn redisInt(h: *TestHarness, argv: []const []const u8) !?i64 {
    var v = try h.queue.command(argv);
    defer v.deinit(h.alloc);
    return switch (v) {
        .integer => |i| i,
        else => null,
    };
}

// ── §0: scaffold ──────────────────────────────────────────────────────────

test "integration: webhook harness — healthz reachable" {
    const alloc = std.testing.allocator;
    const h = startHarness(alloc) catch |err| return skipOrErr(err);
    defer h.deinit();
    const r = try h.get("/healthz").send();
    defer r.deinit();
    try r.expectStatus(.ok);
}

// ── §A: DB-only scenarios (no Redis required) ────────────────────────────

test "A1: invalid HMAC signature → 401 UZ-WH-010" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    // Wrong-secret signature: middleware computes against SECRET, body is signed
    // with a different key — bytes won't match.
    const sig = try signers.signGithub(alloc, "wrong-secret", FAILURE_BODY);
    defer sig.deinit(alloc);
    const r1 = s.h.post(s.url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", "workflow_run");
    const r4 = try r3.header("x-github-delivery", "del_a1");
    const r5 = try r4.json(FAILURE_BODY);
    const r = try r5.send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
    try r.expectErrorCode("UZ-WH-010");
}

test "A2: missing signature header → 401 UZ-WH-010" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const r1 = s.h.post(s.url);
    const r2 = try r1.header("x-github-event", "workflow_run");
    const r3 = try r2.header("x-github-delivery", "del_a2");
    const r4 = try r3.json(FAILURE_BODY);
    const r = try r4.send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
    try r.expectErrorCode("UZ-WH-010");
}

test "A3: wrong X-GitHub-Event → 200 ignored with event name in body" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const r = try postSigned(alloc, &s, "deployment_status", "del_a3", FAILURE_BODY);
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"deployment_status\""));
}

test "A4: body > 1 MiB → 413 UZ-WH-030" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    // Real >1 MiB payload — httpz overrides forged content-length with the
    // actual body length, so the body itself has to cross the cap. The fence
    // catches it via the content-length header before buffering the body.
    const size: usize = 1024 * 1024 + 100;
    const big = try alloc.alloc(u8, size);
    defer alloc.free(big);
    @memset(big, ' ');
    big[0] = '{';
    big[size - 1] = '}';
    const sig = try signers.signGithub(alloc, SECRET, big);
    defer sig.deinit(alloc);
    const r1 = s.h.post(s.url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", "workflow_run");
    const r4 = try r3.header("x-github-delivery", "del_a4");
    const r5 = try r4.json(big);
    const r = try r5.send();
    defer r.deinit();
    try r.expectStatus(.payload_too_large);
    try r.expectErrorCode("UZ-WH-030");
}

test "A5: unknown zombie_id → 404 UZ-WH-001" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    // Build a URL for a different (uninserted) zombie under the same workspace.
    const ghost_url = "/v1/webhooks/0197a4ba-8d3a-7f13-8abc-99999999ffff/github";
    const sig = try signers.signGithub(alloc, SECRET, FAILURE_BODY);
    defer sig.deinit(alloc);
    const r1 = s.h.post(ghost_url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", "workflow_run");
    const r4 = try r3.header("x-github-delivery", "del_a5");
    const r5 = try r4.json(FAILURE_BODY);
    const r = try r5.send();
    defer r.deinit();
    // Either the middleware fails closed (UZ-WH-020 — no credential lookup
    // possible because the zombie row doesn't exist) or the handler 404s
    // (UZ-WH-001). Both are acceptable fail-closed outcomes; we just need to
    // verify it isn't a 202.
    try std.testing.expect(r.status == 401 or r.status == 404);
    try std.testing.expect(r.bodyContains("UZ-WH-001") or r.bodyContains("UZ-WH-020") or r.bodyContains("UZ-WH-010"));
}

test "A6: paused zombie → fail-closed UZ-WH-003" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "paused") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const r = try postSigned(alloc, &s, "workflow_run", "del_a6", FAILURE_BODY);
    defer r.deinit();
    try r.expectErrorCode("UZ-WH-003");
}

test "A7: completed + conclusion=success → 200 ignored non_failure_conclusion" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const r = try postSigned(alloc, &s, "workflow_run", "del_a7", SUCCESS_BODY);
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"non_failure_conclusion\""));
}

test "A8: action=in_progress → 200 ignored non_completed_action" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const r = try postSigned(alloc, &s, "workflow_run", "del_a8", IN_PROGRESS_BODY);
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"non_completed_action\""));
}

test "A11: completed+failure but missing repository → 200 ignored missing_repository (no dedup claim)" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const NO_REPO_BODY =
        \\{"action":"completed","workflow_run":{"id":42,"head_sha":"abc","conclusion":"failure","head_branch":"main","html_url":"u","name":"w","run_attempt":1}}
    ;
    const r = try postSigned(alloc, &s, "workflow_run", "del_a11", NO_REPO_BODY);
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"missing_repository\""));
}

test "A9: 5 successive deployment_status events with distinct deliveries → all 200 ignored, no dedupe interaction" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const deliveries = [_][]const u8{ "del_a9_0", "del_a9_1", "del_a9_2", "del_a9_3", "del_a9_4" };
    for (deliveries) |d| {
        const r = try postSigned(alloc, &s, "deployment_status", d, FAILURE_BODY);
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"ignored\":\"deployment_status\""));
    }
}

// ── §B: Redis-backed scenarios (skip if REDIS_TLS_URL unavailable) ────────

fn requireRedis(h: *TestHarness) !void {
    if (!h.tryConnectRedis()) return error.SkipZigTest;
}

fn xlen(h: *TestHarness, alloc: std.mem.Allocator, zombie_id: []const u8) !i64 {
    const stream = try std.fmt.allocPrint(alloc, "zombie:{s}:events", .{zombie_id});
    defer alloc.free(stream);
    return (try redisInt(h, &.{ "XLEN", stream })) orelse -1;
}

fn dedupTtl(h: *TestHarness, alloc: std.mem.Allocator, zombie_id: []const u8, delivery: []const u8) !i64 {
    const key = try std.fmt.allocPrint(alloc, "webhook:dedup:{s}:gh:{s}", .{ zombie_id, delivery });
    defer alloc.free(key);
    return (try redisInt(h, &.{ "TTL", key })) orelse -2;
}

fn cleanupRedis(h: *TestHarness, alloc: std.mem.Allocator, zombie_id: []const u8, deliveries: []const []const u8) void {
    const stream = std.fmt.allocPrint(alloc, "zombie:{s}:events", .{zombie_id}) catch return;
    defer alloc.free(stream);
    var v = h.queue.command(&.{ "DEL", stream }) catch return;
    v.deinit(alloc);
    for (deliveries) |d| {
        const k = std.fmt.allocPrint(alloc, "webhook:dedup:{s}:gh:{s}", .{ zombie_id, d }) catch continue;
        defer alloc.free(k);
        var v2 = h.queue.command(&.{ "DEL", k }) catch continue;
        v2.deinit(alloc);
    }
}

test "B1: happy path — 202; dedup key set with ~72h TTL; XLEN += 1" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    // Pre-clean stale state from any previously-aborted run; the deferred
    // post-clean only fires on this test's own exit, so a crash in an
    // earlier session can leave dedup keys / stream entries that flake the
    // next assertion. Idempotent: DEL on a missing key is a Redis no-op.
    cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b1"});
    defer cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b1"});

    const before = try xlen(s.h, alloc, s.fx.zombie_id);
    const r = try postSigned(alloc, &s, "workflow_run", "del_b1", FAILURE_BODY);
    defer r.deinit();

    try r.expectStatus(.accepted);
    try std.testing.expect(r.bodyContains("\"event_id\""));
    const after = try xlen(s.h, alloc, s.fx.zombie_id);
    try std.testing.expectEqual(before + 1, after);
    const ttl = try dedupTtl(s.h, alloc, s.fx.zombie_id, "del_b1");
    try std.testing.expect(ttl > 259195 and ttl <= 259200);
}

test "B2: replay same X-GitHub-Delivery → first 202, second 200 deduped; XLEN += 1 only" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b2"});
    defer cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b2"});

    const before = try xlen(s.h, alloc, s.fx.zombie_id);
    const r1 = try postSigned(alloc, &s, "workflow_run", "del_b2", FAILURE_BODY);
    defer r1.deinit();
    try r1.expectStatus(.accepted);

    const r2 = try postSigned(alloc, &s, "workflow_run", "del_b2", FAILURE_BODY);
    defer r2.deinit();
    try r2.expectStatus(.ok);
    try std.testing.expect(r2.bodyContains("\"deduped\":true"));

    const after = try xlen(s.h, alloc, s.fx.zombie_id);
    try std.testing.expectEqual(before + 1, after); // dedupe blocked the second XADD
}

test "B3: 5 concurrent POSTs same delivery → exactly one 202; XLEN += 1" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b3"});
    defer cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b3"});

    const N = 5;
    var threads: [N]std.Thread = undefined;
    var statuses: [N]u16 = .{0} ** N;
    const Worker = struct {
        fn run(a: std.mem.Allocator, setup: *Setup, slot: *u16) void {
            const r = postSigned(a, setup, "workflow_run", "del_b3", FAILURE_BODY) catch {
                slot.* = 0;
                return;
            };
            defer r.deinit();
            slot.* = r.status;
        }
    };
    const before = try xlen(s.h, alloc, s.fx.zombie_id);
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ alloc, &s, &statuses[i] });
    }
    for (threads) |t| t.join();
    const after = try xlen(s.h, alloc, s.fx.zombie_id);

    var accepted_count: usize = 0;
    var deduped_or_ok_count: usize = 0;
    for (statuses) |st| {
        if (st == 202) accepted_count += 1;
        if (st == 200) deduped_or_ok_count += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), accepted_count);
    try std.testing.expectEqual(@as(usize, N - 1), deduped_or_ok_count);
    try std.testing.expectEqual(before + 1, after);
}

test "B4: credential_name override resolves to alternate vault key → 202" {
    const alloc = std.testing.allocator;
    const h = startHarness(alloc) catch |err| return skipOrErr(err);
    defer h.deinit();
    requireRedis(h) catch return error.SkipZigTest;

    const fx: fx_mod.Fixture = .{
        .tenant_id = fx_mod.ID_TENANT_A,
        .workspace_id = fx_mod.ID_WS_A,
        .zombie_id = fx_mod.ID_ZOMBIE_A,
    };
    // Trigger pins credential_name="github-prod"; default would be "github".
    const trigger = try fx_mod.buildTriggerConfig(alloc, "github", "github-prod");
    defer alloc.free(trigger);
    const override_secret = "override-key-abc";

    const conn = try h.acquireConn();
    try fx_mod.insertZombie(conn, fx, trigger);
    // Insert the alternate credential at the override name; do NOT insert
    // one at the default name — proves the override is what got resolved.
    try fx_mod.insertWebhookCredential(alloc, conn, fx.workspace_id, "github-prod", override_secret);
    h.releaseConn(conn);
    defer {
        const cc = h.acquireConn() catch null;
        if (cc) |c| {
            fx_mod.cleanup(c, fx) catch {};
            h.releaseConn(c);
        }
    }
    cleanupRedis(h, alloc, fx.zombie_id, &.{"del_b4"});
    defer cleanupRedis(h, alloc, fx.zombie_id, &.{"del_b4"});

    const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}/github", .{fx.zombie_id});
    defer alloc.free(url);
    const sig = try signers.signGithub(alloc, override_secret, FAILURE_BODY);
    defer sig.deinit(alloc);
    const r1 = h.post(url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", "workflow_run");
    const r4 = try r3.header("x-github-delivery", "del_b4");
    const r5 = try r4.json(FAILURE_BODY);
    const r = try r5.send();
    defer r.deinit();
    try r.expectStatus(.accepted);
}

test "B5: filter-rejected delivery does NOT claim dedup slot — replay with valid filter still 202s" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b5"});
    defer cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b5"});

    // First POST: filter-rejected (success conclusion). Must NOT claim slot.
    const r1 = try postSigned(alloc, &s, "workflow_run", "del_b5", SUCCESS_BODY);
    defer r1.deinit();
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("\"ignored\":\"non_failure_conclusion\""));

    // Verify dedup key was NOT set: TTL returns -2 for a missing key.
    const ttl_after_filter = try dedupTtl(s.h, alloc, s.fx.zombie_id, "del_b5");
    try std.testing.expectEqual(@as(i64, -2), ttl_after_filter);

    // Second POST: same delivery UUID, valid failure conclusion → must 202.
    // If dedupe were claimed before filter (the M43 pre-amendment ordering),
    // this would dedupe and skip XADD — silent data loss.
    const before = try xlen(s.h, alloc, s.fx.zombie_id);
    const r2 = try postSigned(alloc, &s, "workflow_run", "del_b5", FAILURE_BODY);
    defer r2.deinit();
    try r2.expectStatus(.accepted);
    const after = try xlen(s.h, alloc, s.fx.zombie_id);
    try std.testing.expectEqual(before + 1, after);
}

test "B6: TTL on accepted dedup key falls within 5s of 72h" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b6"});
    defer cleanupRedis(s.h, alloc, s.fx.zombie_id, &.{"del_b6"});

    const r = try postSigned(alloc, &s, "workflow_run", "del_b6", FAILURE_BODY);
    defer r.deinit();
    try r.expectStatus(.accepted);

    const ttl = try dedupTtl(s.h, alloc, s.fx.zombie_id, "del_b6");
    // 72h = 259200s. Accept anything within the last 5 seconds (test latency).
    try std.testing.expect(ttl >= 259195);
    try std.testing.expect(ttl <= 259200);
}
