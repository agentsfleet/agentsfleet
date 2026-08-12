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

const harness_mod = @import("test_harness.zig");
const fx_mod = @import("webhook_test_fixtures.zig");
const signers = @import("webhook_test_signers.zig");
const whc = @import("../fleet_runtime/webhook_constants.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const TestHarness = harness_mod.TestHarness;

// ── Middleware wiring ─────────────────────────────────────────────────────

// SAFETY: test fixture; field is populated by the surrounding builder before any read.
var wired_webhook_sig: webhook_sig.WebhookSig(*pg.Pool) = undefined;
// SAFETY: test fixture; field is populated by the surrounding builder before any read.
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
const PULL_REQUEST_BODY =
    \\{"action":"opened","number":42,"repository":{"full_name":"o/r"},"pull_request":{"number":42,"title":"Fix routing","html_url":"https://github.com/o/r/pull/42","state":"open","draft":false,"user":{"login":"indy"},"head":{"ref":"fix","sha":"abc123"},"base":{"ref":"main"}}}
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
            .fleet_id = fx_mod.ID_AGENTSFLEET_A,
        };
        const trigger = try fx_mod.buildTriggerConfig(alloc, "github", null);
        defer alloc.free(trigger);
        const conn = try h.acquireConn();
        try fx_mod.insertFleet(conn, fx, trigger);
        try fx_mod.insertWebhookCredential(alloc, conn, fx.workspace_id, "github", SECRET);
        if (!std.mem.eql(u8, status, "active")) {
            _ = try conn.exec("UPDATE core.fleets SET status = $1 WHERE id = $2::uuid", .{ status, fx.fleet_id });
        }
        h.releaseConn(conn);
        const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}/github", .{fx.fleet_id});
        return .{ .h = h, .fx = fx, .url = url };
    }

    fn deinit(self: *Setup, alloc: std.mem.Allocator) void {
        const conn = self.h.acquireConn() catch null;
        if (conn) |c| {
            fx_mod.cleanup(c, self.fx) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
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
    return r5.send();
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

test "A5: unknown fleet_id → 404 UZ-WH-001" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    // Build a URL for a different (uninserted) fleet under the same workspace.
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
    // possible because the fleet row doesn't exist) or the handler 404s
    // (UZ-WH-001). Both are acceptable fail-closed outcomes; we just need to
    // verify it isn't a 202.
    try std.testing.expect(r.status == 401 or r.status == 404);
    try std.testing.expect(r.bodyContains("UZ-WH-001") or r.bodyContains("UZ-WH-020") or r.bodyContains("UZ-WH-010"));
}

test "A5b: an uppercase fleet_id is rejected before it can split the dedup key" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    // The SAME fleet as the happy path, spelled in uppercase. Postgres folds
    // case on `WHERE id = $1::uuid`, so this used to resolve to the real fleet
    // — but the Redis dedup key is built from the raw path value, so the same
    // GitHub delivery under two spellings claimed two slots and was processed
    // twice. Without the shape guard this returns 202, not 400.
    var upper_buf: [36]u8 = undefined;
    const upper = std.ascii.upperString(&upper_buf, s.fx.fleet_id);
    const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}/github", .{upper});
    defer alloc.free(url);
    const sig = try signers.signGithub(alloc, SECRET, FAILURE_BODY);
    defer sig.deinit(alloc);
    const r1 = s.h.post(url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.header("x-github-event", "workflow_run");
    const r4 = try r3.header("x-github-delivery", "del_a5b");
    const r5 = try r4.json(FAILURE_BODY);
    const r = try r5.send();
    defer r.deinit();
    // Fail-closed either way: the handler's shape guard (400 UZ-UUIDV7-009) or
    // the middleware refusing a credential lookup it cannot resolve. What must
    // NOT happen is a 202 accept, which is the double-delivery path.
    try std.testing.expect(r.status != 202);
    try std.testing.expect(r.bodyContains("UZ-UUIDV7-009") or r.bodyContains("UZ-WH-020") or r.bodyContains("UZ-WH-010"));
}

test "A6: paused fleet → 200 ignored fleet_paused, trigger metric unchanged" {
    const metrics_fleet = @import("../observability/metrics_fleet.zig");
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "paused") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const triggered_before = metrics_fleet.snapshotFleetFields().fleet_triggered_total;
    const r = try postSigned(alloc, &s, "workflow_run", "del_a6", FAILURE_BODY);
    defer r.deinit();
    // 200-ignored (not 4xx) so sender retry queues stay quiet for
    // an intentionally paused fleet; nothing accepted → metric unchanged.
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"fleet_paused\""));
    try std.testing.expectEqual(triggered_before, metrics_fleet.snapshotFleetFields().fleet_triggered_total);
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

fn xlen(h: *TestHarness, alloc: std.mem.Allocator, fleet_id: []const u8) !i64 {
    const stream = try std.fmt.allocPrint(alloc, "fleet:{s}:events", .{fleet_id});
    defer alloc.free(stream);
    return (try redisInt(h, &.{ "XLEN", stream })) orelse -1;
}

fn dedupTtl(h: *TestHarness, alloc: std.mem.Allocator, fleet_id: []const u8, delivery: []const u8) !i64 {
    const key = try std.fmt.allocPrint(alloc, "webhook:dedup:{s}:gh:{s}", .{ fleet_id, delivery });
    defer alloc.free(key);
    return (try redisInt(h, &.{ "TTL", key })) orelse -2;
}

/// Drop the fleet's whole Redis footprint the way production does when a fleet
/// stops being leasable. Teardown only — the `DEL`/`SET` pairs inside the C1 and
/// B7 tests are deliberate WRONGTYPE fault injection and must stay as they are.
fn forgetFleet(h: *TestHarness, fleet_id: []const u8) void {
    redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupRedis(h: *TestHarness, alloc: std.mem.Allocator, fleet_id: []const u8, deliveries: []const []const u8) void {
    // Stream, readiness mark, and group memo together. A bare stream DEL leaves
    // the mark stranded in the one deployment-wide index, where a later suite's
    // poll can draw this fleet out of the random sample instead of its own. The
    // dedup keys below are this suite's alone and are not part of that state.
    forgetFleet(h, fleet_id);
    for (deliveries) |d| {
        const k = std.fmt.allocPrint(alloc, "webhook:dedup:{s}:gh:{s}", .{ fleet_id, d }) catch continue;
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
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b1"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b1"});

    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const r = try postSigned(alloc, &s, "workflow_run", "del_b1", FAILURE_BODY);
    defer r.deinit();

    try r.expectStatus(.accepted);
    try std.testing.expect(r.bodyContains("\"event_id\""));
    const after = try xlen(s.h, alloc, s.fx.fleet_id);
    try std.testing.expectEqual(before + 1, after);
    const ttl = try dedupTtl(s.h, alloc, s.fx.fleet_id, "del_b1");
    try std.testing.expect(ttl > 259195 and ttl <= 259200);
}

test "B8: opened pull request reaches the per-fleet event stream" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b8"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b8"});

    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const response = try postSigned(alloc, &s, "pull_request", "del_b8", PULL_REQUEST_BODY);
    defer response.deinit();

    try response.expectStatus(.accepted);
    try std.testing.expect(response.bodyContains("\"event_id\""));
    try std.testing.expectEqual(before + 1, try xlen(s.h, alloc, s.fx.fleet_id));
}

test "B2: replay same X-GitHub-Delivery → first 202, second 200 deduped; XLEN += 1 only" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b2"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b2"});

    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const r1 = try postSigned(alloc, &s, "workflow_run", "del_b2", FAILURE_BODY);
    defer r1.deinit();
    try r1.expectStatus(.accepted);

    const r2 = try postSigned(alloc, &s, "workflow_run", "del_b2", FAILURE_BODY);
    defer r2.deinit();
    try r2.expectStatus(.ok);
    try std.testing.expect(r2.bodyContains("\"deduped\":true"));

    const after = try xlen(s.h, alloc, s.fx.fleet_id);
    try std.testing.expectEqual(before + 1, after); // dedupe blocked the second XADD
}

test "B3: 5 concurrent POSTs same delivery → exactly one 202; XLEN += 1" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b3"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b3"});

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
    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ alloc, &s, &statuses[i] });
    }
    for (threads) |t| t.join();
    const after = try xlen(s.h, alloc, s.fx.fleet_id);

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
        .fleet_id = fx_mod.ID_AGENTSFLEET_A,
    };
    // Trigger pins credential_name="github-prod"; default would be "github".
    const trigger = try fx_mod.buildTriggerConfig(alloc, "github", "github-prod");
    defer alloc.free(trigger);
    const override_secret = "override-key-abc";

    const conn = try h.acquireConn();
    try fx_mod.insertFleet(conn, fx, trigger);
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
    cleanupRedis(h, alloc, fx.fleet_id, &.{"del_b4"});
    defer cleanupRedis(h, alloc, fx.fleet_id, &.{"del_b4"});

    const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}/github", .{fx.fleet_id});
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
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b5"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b5"});

    // First POST: filter-rejected (success conclusion). Must NOT claim slot.
    const r1 = try postSigned(alloc, &s, "workflow_run", "del_b5", SUCCESS_BODY);
    defer r1.deinit();
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("\"ignored\":\"non_failure_conclusion\""));

    // Verify dedup key was NOT set: TTL returns -2 for a missing key.
    const ttl_after_filter = try dedupTtl(s.h, alloc, s.fx.fleet_id, "del_b5");
    try std.testing.expectEqual(@as(i64, -2), ttl_after_filter);

    // Second POST: same delivery UUID, valid failure conclusion → must 202.
    // If dedupe were claimed before filter (the M43 pre-amendment ordering),
    // this would dedupe and skip XADD — silent data loss.
    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const r2 = try postSigned(alloc, &s, "workflow_run", "del_b5", FAILURE_BODY);
    defer r2.deinit();
    try r2.expectStatus(.accepted);
    const after = try xlen(s.h, alloc, s.fx.fleet_id);
    try std.testing.expectEqual(before + 1, after);
}

test "B6: TTL on accepted dedup key falls within 5s of 72h" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b6"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b6"});

    const r = try postSigned(alloc, &s, "workflow_run", "del_b6", FAILURE_BODY);
    defer r.deinit();
    try r.expectStatus(.accepted);

    const ttl = try dedupTtl(s.h, alloc, s.fx.fleet_id, "del_b6");
    // 72h = 259200s. Accept anything within the last 5 seconds (test latency).
    try std.testing.expect(ttl >= 259195);
    try std.testing.expect(ttl <= 259200);
}

test "B7: enqueue failure releases the dedup slot — retry of the same delivery enqueues exactly one event" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b7"});
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{"del_b7"});

    const stream_key = try std.fmt.allocPrint(alloc, "fleet:{s}:events", .{s.fx.fleet_id});
    defer alloc.free(stream_key);

    // Inject the enqueue fault (loss-proof dedup ordering): park a plain
    // string at the stream key so XADD answers WRONGTYPE — a real server-side
    // enqueue failure, no seam needed.
    {
        var del = try s.h.queue.commandAllowError(&.{ "DEL", stream_key });
        del.deinit(s.h.queue.alloc);
        var set = try s.h.queue.commandAllowError(&.{ "SET", stream_key, "fault" });
        set.deinit(s.h.queue.alloc);
    }
    const r1 = try postSigned(alloc, &s, "workflow_run", "del_b7", FAILURE_BODY);
    defer r1.deinit();
    try r1.expectStatus(.internal_server_error);

    // Clear the fault; the sender retries the SAME delivery UUID — the slot
    // was released, so the retry delivers (not "deduped"), exactly once.
    {
        var del = try s.h.queue.commandAllowError(&.{ "DEL", stream_key });
        del.deinit(s.h.queue.alloc);
    }
    const r2 = try postSigned(alloc, &s, "workflow_run", "del_b7", FAILURE_BODY);
    defer r2.deinit();
    try r2.expectStatus(.accepted);
    try std.testing.expectEqual(@as(i64, 1), try xlen(s.h, alloc, s.fx.fleet_id));
}

// ── §C: generic-route twin — fleet.zig carries its own copy of the
// loss-proof dedup ordering (claim → enqueue, release on failure), so the
// injection proof runs against the generic `/v1/webhooks/{id}` route too,
// signed with the linear scheme (bare-hex HMAC, no prefix). ──────────────

const AGENTSFLEET_LINEAR = "0197a4ba-8d3a-7f13-8abc-11111111aa31";
const LINEAR_SECRET = "topsecret-linear-key";
const LINEAR_EVENT_ID = "lin_c1";
const LINEAR_BODY =
    \\{"event_id":"lin_c1","type":"issue.updated","data":{"k":"v"}}
;

fn linearSetup(alloc: std.mem.Allocator, status: []const u8) !Setup {
    const h = try startHarness(alloc);
    errdefer h.deinit();
    const fx: fx_mod.Fixture = .{
        .tenant_id = fx_mod.ID_TENANT_A,
        .workspace_id = fx_mod.ID_WS_A,
        .fleet_id = AGENTSFLEET_LINEAR,
    };
    const trigger = try fx_mod.buildTriggerConfig(alloc, "linear", null);
    defer alloc.free(trigger);
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try fx_mod.insertFleet(conn, fx, trigger);
    try fx_mod.insertWebhookCredential(alloc, conn, fx.workspace_id, "linear", LINEAR_SECRET);
    if (!std.mem.eql(u8, status, "active")) {
        _ = try conn.exec("UPDATE core.fleets SET status = $1 WHERE id = $2::uuid", .{ status, fx.fleet_id });
    }
    const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}", .{fx.fleet_id});
    return .{ .h = h, .fx = fx, .url = url };
}

fn postSignedLinear(alloc: std.mem.Allocator, s: *Setup, body: []const u8) !harness_mod.Response {
    const sig = try signers.signLinear(alloc, LINEAR_SECRET, body);
    defer sig.deinit(alloc);
    const r1 = s.h.post(s.url);
    const r2 = try r1.header(sig.header_name, sig.header_value);
    const r3 = try r2.json(body);
    return r3.send();
}

// Generic-route dedup key carries no provider segment: webhook:dedup:{zid}:{event_id}.
fn cleanupLinearRedis(h: *TestHarness, alloc: std.mem.Allocator) void {
    forgetFleet(h, AGENTSFLEET_LINEAR);
    const k = std.fmt.allocPrint(alloc, "{s}{s}:{s}", .{ whc.WEBHOOK_DEDUP_KEY_PREFIX, AGENTSFLEET_LINEAR, LINEAR_EVENT_ID }) catch return;
    defer alloc.free(k);
    var v2 = h.queue.commandAllowError(&.{ "DEL", k }) catch return;
    v2.deinit(h.queue.alloc);
}

test "C1: generic route — enqueue failure releases the dedup slot; retry delivers once; replay dedupes" {
    const alloc = std.testing.allocator;
    var s = linearSetup(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupLinearRedis(s.h, alloc);
    defer cleanupLinearRedis(s.h, alloc);

    const stream_key = try std.fmt.allocPrint(alloc, "fleet:{s}:events", .{AGENTSFLEET_LINEAR});
    defer alloc.free(stream_key);

    // Inject the enqueue fault: park a plain string at the stream key so
    // XADD answers WRONGTYPE — a real server-side failure, no seam needed.
    {
        var del = try s.h.queue.commandAllowError(&.{ "DEL", stream_key });
        del.deinit(s.h.queue.alloc);
        var set = try s.h.queue.commandAllowError(&.{ "SET", stream_key, "fault" });
        set.deinit(s.h.queue.alloc);
    }
    const r1 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r1.deinit();
    try r1.expectStatus(.internal_server_error);

    // Clear the fault; the sender retries the SAME event_id — the slot was
    // released, so the retry delivers (not "duplicate"), exactly once.
    {
        var del = try s.h.queue.commandAllowError(&.{ "DEL", stream_key });
        del.deinit(s.h.queue.alloc);
    }
    const r2 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r2.deinit();
    try r2.expectStatus(.accepted);
    try std.testing.expectEqual(@as(i64, 1), try xlen(s.h, alloc, AGENTSFLEET_LINEAR));

    // Replay after success → deduped, stream unchanged (generic-side 3.2 pin).
    const r3 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r3.deinit();
    try r3.expectStatus(.ok);
    try std.testing.expect(r3.bodyContains("\"status\":\"duplicate\""));
    try std.testing.expectEqual(@as(i64, 1), try xlen(s.h, alloc, AGENTSFLEET_LINEAR));
}

test "C2: generic route — paused fleet → 200 ignored fleet_paused, dedup slot not consumed" {
    const metrics_fleet = @import("../observability/metrics_fleet.zig");
    const alloc = std.testing.allocator;
    var s = linearSetup(alloc, "paused") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupLinearRedis(s.h, alloc);
    defer cleanupLinearRedis(s.h, alloc);

    const triggered_before = metrics_fleet.snapshotFleetFields().fleet_triggered_total;
    const r1 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r1.deinit();
    // 200-ignored (not 4xx): sender retry queues add no value for an
    // intentionally paused fleet; nothing accepted → trigger metric unchanged.
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("\"ignored\":\"fleet_paused\""));
    try std.testing.expectEqual(triggered_before, metrics_fleet.snapshotFleetFields().fleet_triggered_total);

    // The dedup slot was not consumed: after resume, the SAME event_id
    // delivers exactly one event (an operator redelivery still works).
    {
        const conn = try s.h.acquireConn();
        defer s.h.releaseConn(conn);
        _ = try conn.exec("UPDATE core.fleets SET status = 'active' WHERE id = $1::uuid", .{AGENTSFLEET_LINEAR});
    }
    const r2 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r2.deinit();
    try r2.expectStatus(.accepted);
    try std.testing.expectEqual(@as(i64, 1), try xlen(s.h, alloc, AGENTSFLEET_LINEAR));
}

// ── §R: repair-branch linkage arms ────────────────────────────────────────

const repair_branch = @import("../git/repair_branch.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");

const REPAIR_INCIDENT_EVENT = "evt-incident-77";
const REPAIR_GATE_ID = "0197a4ba-8d3a-7f13-8abc-33333333cc31";
const UNKNOWN_REPAIR_GATE_ID = "0197a4ba-8d3a-7f13-8abc-33333333cc32";
const REPAIR_INSTALL_ID = "42";
const REPAIR_CONNECTOR_ID = "0197a4ba-8d3a-7f13-8abc-33333333cc33";
const REPAIR_BRANCH = repair_branch.fromGateId(REPAIR_GATE_ID) catch @panic("fixed repair gate identifier must encode");
const UNKNOWN_REPAIR_BRANCH = repair_branch.fromGateId(UNKNOWN_REPAIR_GATE_ID) catch @panic("fixed unknown repair gate identifier must encode");
const REPAIR_REPO = "o/r";
const REPAIR_BINDING = "{\"repositories\":[\"o/r\"],\"access\":\"write\",\"base\":\"main\"}";
const REPAIR_MERGE_SHA = "0123456789abcdef0123456789abcdef01234567";
const CONFLICTING_REPAIR_MERGE_SHA = "89abcdef0123456789abcdef0123456789abcdef";
const OLD_DAEMON_DEPLOY_OK = "deploy_ok";
const OLD_DAEMON_DEPLOY_FAILED = "deploy_failed";
const OLD_DAEMON_STAMP_REPAIR_PR_DEPLOY =
    \\UPDATE core.repair_pr_links
    \\SET deploy_status = $4, deploy_stamped_at = $5
    \\WHERE fleet_id = $1::uuid AND branch = $2 AND repository = $3
;

fn repairPrBody(alloc: std.mem.Allocator, branch: []const u8, action: []const u8, author: []const u8, fork: bool, merged: bool, merge_sha: []const u8) ![]u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"action\":\"{s}\",\"installation\":{{\"id\":42}},\"repository\":{{\"full_name\":\"o/r\"}},\"pull_request\":{{\"number\":88,\"html_url\":\"https://github.com/o/r/pull/88\",\"user\":{{\"login\":\"{s}\"}},\"head\":{{\"ref\":\"{s}\",\"repo\":{{\"full_name\":\"o/r\",\"fork\":{s}}}}},\"base\":{{\"ref\":\"main\",\"repo\":{{\"full_name\":\"o/r\"}}}},\"merged\":{s},\"merge_commit_sha\":\"{s}\",\"merged_at\":\"2026-08-10T12:00:00Z\"}}}}",
        .{ action, author, branch, if (fork) "true" else "false", if (merged) "true" else "false", merge_sha },
    );
}

fn repairRunBody(alloc: std.mem.Allocator, branch: []const u8, run_id: i64, name: []const u8, sha: []const u8, conclusion: []const u8) ![]u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"action\":\"completed\",\"installation\":{{\"id\":42}},\"workflow_run\":{{\"id\":{d},\"name\":\"{s}\",\"head_sha\":\"{s}\",\"conclusion\":\"{s}\",\"updated_at\":\"2026-08-10T12:01:00Z\",\"head_branch\":\"{s}\"}},\"repository\":{{\"full_name\":\"o/r\"}}}}",
        .{ run_id, name, sha, conclusion, branch },
    );
}

fn seedRepairAuthority(s: *Setup, status: []const u8) !void {
    s.h.ctx.github_app_slug = "agentsfleet";
    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    _ = try conn.exec(
        \\INSERT INTO core.connector_installs
        \\  (id, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
        \\VALUES ($1::uuid, 'github', $2, $3::uuid, 'test', ARRAY[]::text[], 1, 1)
        \\ON CONFLICT (provider, external_account_id) DO UPDATE SET workspace_id = EXCLUDED.workspace_id
    , .{ REPAIR_CONNECTOR_ID, REPAIR_INSTALL_ID, s.fx.workspace_id });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'webhook:github', 'webhook', 'received',
        \\        '{}'::jsonb, 1, 1)
        \\ON CONFLICT (fleet_id, event_id) DO NOTHING
    , .{ s.fx.fleet_id, s.fx.workspace_id, REPAIR_INCIDENT_EVENT });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   resolved_by, status, detail, created_at, updated_at, event_id,
        \\   stated_binding, spend_count, spend_ceiling)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'repair-action', 'github', 'write',
        \\        $4, '', '{}'::jsonb, '', 9999999999999,
        \\        'indy', $5, '', 1, 2, $6, $7::jsonb, 0, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ REPAIR_GATE_ID, s.fx.fleet_id, s.fx.workspace_id, gate_constants.GATE_KIND_REPOSITORY_WRITE, status, REPAIR_INCIDENT_EVENT, REPAIR_BINDING, gate_constants.REPOSITORY_WRITE_SPEND_CEILING });
}

/// Assert the one linkage row for this incident. Row slices live only until the
/// query is released, so the assertions happen inside its scope rather than
/// duplicating the row out to the caller.
fn expectLink(s: *Setup, merged_sha: ?[]const u8) !void {
    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT repository, pr_number, merged_commit_sha, merged_at
        \\FROM core.repair_pr_links
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ s.fx.fleet_id, REPAIR_INCIDENT_EVENT }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(REPAIR_REPO, try row.get([]const u8, 0));
    try std.testing.expectEqual(@as(i64, 88), try row.get(i64, 1));
    const stored_sha = try row.get(?[]const u8, 2);
    if (merged_sha) |want| try std.testing.expectEqualStrings(want, stored_sha.?) else try std.testing.expect(stored_sha == null);
    try std.testing.expectEqual(merged_sha != null, (try row.get(?i64, 3)) != null);
}

fn repairRunCount(s: *Setup) !i64 {
    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT count(*) FROM core.repair_run_results WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return try row.get(i64, 0);
}

fn repairLinkCount(s: *Setup) !i64 {
    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT count(*) FROM core.repair_pr_links WHERE workspace_id = $1::uuid",
        .{s.fx.workspace_id},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return try row.get(i64, 0);
}

test "test_own_repair_pr_links_without_waking_fleet" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    try seedRepairAuthority(&s, "approved");
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r1", "del_r2" });
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r1", "del_r2" });

    const body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(body);
    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const r1 = try postSigned(alloc, &s, "pull_request", "del_r1", body);
    defer r1.deinit();
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("linked"));
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));
    try expectLink(&s, null);

    const r2 = try postSigned(alloc, &s, "pull_request", "del_r2", body);
    defer r2.deinit();
    try r2.expectStatus(.ok);
    try std.testing.expect(r2.bodyContains("duplicate_repair_link"));
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));
}

test "test_repair_runs_append_before_pull_request_and_replay_once" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    try seedRepairAuthority(&s, "approved");
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r3", "del_r4", "del_r5", "del_r6", "del_r7" });
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r3", "del_r4", "del_r5", "del_r6", "del_r7" });
    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const runs = [_]struct { id: i64, name: []const u8, sha: []const u8, conclusion: []const u8 }{
        .{ .id = 43, .name = "lint", .sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", .conclusion = "success" },
        .{ .id = 44, .name = "test", .sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", .conclusion = "failure" },
        .{ .id = 45, .name = "preview", .sha = "cccccccccccccccccccccccccccccccccccccccc", .conclusion = "cancelled" },
    };
    inline for (runs, 0..) |run, index| {
        const body = try repairRunBody(alloc, &REPAIR_BRANCH, run.id, run.name, run.sha, run.conclusion);
        defer alloc.free(body);
        const delivery = try std.fmt.allocPrint(alloc, "del_r{d}", .{index + 3});
        defer alloc.free(delivery);
        const response = try postSigned(alloc, &s, "workflow_run", delivery, body);
        defer response.deinit();
        try response.expectStatus(.ok);
        try std.testing.expect(response.bodyContains("repair_run_recorded"));
    }
    try std.testing.expectEqual(@as(i64, 3), try repairRunCount(&s));

    const replay_body = try repairRunBody(alloc, &REPAIR_BRANCH, 43, "lint", runs[0].sha, "success");
    defer alloc.free(replay_body);
    const replay = try postSigned(alloc, &s, "workflow_run", "del_r6", replay_body);
    defer replay.deinit();
    try replay.expectStatus(.ok);
    try std.testing.expect(replay.bodyContains("repair_run_replayed"));
    try std.testing.expectEqual(@as(i64, 3), try repairRunCount(&s));

    const pr_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(pr_body);
    const linked = try postSigned(alloc, &s, "pull_request", "del_r7", pr_body);
    defer linked.deinit();
    try linked.expectStatus(.ok);
    try expectLink(&s, null);
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));
}

test "test_merged_pull_request_records_exact_provider_hash_once" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "approved");

    const open_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(open_body);
    const opened = try postSigned(alloc, &s, "pull_request", "del_r8", open_body);
    defer opened.deinit();
    try opened.expectStatus(.ok);

    const merge_body = try repairPrBody(alloc, &REPAIR_BRANCH, "closed", "agentsfleet[bot]", false, true, REPAIR_MERGE_SHA);
    defer alloc.free(merge_body);
    const merged = try postSigned(alloc, &s, "pull_request", "del_r9", merge_body);
    defer merged.deinit();
    try merged.expectStatus(.ok);
    try std.testing.expect(merged.bodyContains(REPAIR_MERGE_SHA));
    try expectLink(&s, REPAIR_MERGE_SHA);

    const replay = try postSigned(alloc, &s, "pull_request", "del_r10", merge_body);
    defer replay.deinit();
    try replay.expectStatus(.ok);
    try std.testing.expect(replay.bodyContains(REPAIR_MERGE_SHA));
    try expectLink(&s, REPAIR_MERGE_SHA);

    const conflicting_body = try repairPrBody(
        alloc,
        &REPAIR_BRANCH,
        "closed",
        "agentsfleet[bot]",
        false,
        true,
        CONFLICTING_REPAIR_MERGE_SHA,
    );
    defer alloc.free(conflicting_body);
    const conflicting = try postSigned(alloc, &s, "pull_request", "del_r10_conflict", conflicting_body);
    defer conflicting.deinit();
    try conflicting.expectStatus(.ok);
    try std.testing.expect(conflicting.bodyContains("unmerged_repair_pr"));
    try expectLink(&s, REPAIR_MERGE_SHA);
}

test "test_new_schema_accepts_old_daemon_deploy_stamp_during_rolling_replacement" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "approved");

    const open_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(open_body);
    const opened = try postSigned(alloc, &s, "pull_request", "del_old_daemon_open", open_body);
    defer opened.deinit();
    try opened.expectStatus(.ok);

    {
        const conn = try s.h.acquireConn();
        defer s.h.releaseConn(conn);
        _ = try conn.exec(OLD_DAEMON_STAMP_REPAIR_PR_DEPLOY, .{
            s.fx.fleet_id,
            &REPAIR_BRANCH,
            REPAIR_REPO,
            OLD_DAEMON_DEPLOY_OK,
            @as(i64, 1770000000000),
        });
    }

    const merge_body = try repairPrBody(alloc, &REPAIR_BRANCH, "closed", "agentsfleet[bot]", false, true, REPAIR_MERGE_SHA);
    defer alloc.free(merge_body);
    const merged = try postSigned(alloc, &s, "pull_request", "del_old_daemon_merge", merge_body);
    defer merged.deinit();
    try merged.expectStatus(.ok);

    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    _ = try conn.exec(OLD_DAEMON_STAMP_REPAIR_PR_DEPLOY, .{
        s.fx.fleet_id,
        &REPAIR_BRANCH,
        REPAIR_REPO,
        OLD_DAEMON_DEPLOY_FAILED,
        @as(i64, 1770000000001),
    });
    var q = PgQuery.from(try conn.query(
        \\SELECT deploy_status, deploy_stamped_at, merged_commit_sha
        \\FROM core.repair_pr_links
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ s.fx.fleet_id, REPAIR_INCIDENT_EVENT }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(OLD_DAEMON_DEPLOY_FAILED, try row.get([]const u8, 0));
    try std.testing.expectEqual(@as(i64, 1770000000001), try row.get(i64, 1));
    try std.testing.expectEqualStrings(REPAIR_MERGE_SHA, try row.get([]const u8, 2));
}

test "test_unmerged_or_hashless_pull_request_never_records_merge" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "approved");

    const open_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(open_body);
    const opened = try postSigned(alloc, &s, "pull_request", "del_unmerged_open", open_body);
    defer opened.deinit();
    try opened.expectStatus(.ok);
    try expectLink(&s, null);

    const unmerged_body = try repairPrBody(alloc, &REPAIR_BRANCH, "closed", "agentsfleet[bot]", false, false, REPAIR_MERGE_SHA);
    defer alloc.free(unmerged_body);
    const unmerged = try postSigned(alloc, &s, "pull_request", "del_unmerged_close", unmerged_body);
    defer unmerged.deinit();
    try unmerged.expectStatus(.ok);
    try std.testing.expect(unmerged.bodyContains("unmerged_repair_pr"));
    try expectLink(&s, null);

    const hashless_body = try repairPrBody(alloc, &REPAIR_BRANCH, "closed", "agentsfleet[bot]", false, true, "");
    defer alloc.free(hashless_body);
    const hashless = try postSigned(alloc, &s, "pull_request", "del_hashless_close", hashless_body);
    defer hashless.deinit();
    try hashless.expectStatus(.ok);
    try std.testing.expect(hashless.bodyContains("unmerged_repair_pr"));
    try expectLink(&s, null);
}

test "test_invalid_or_unapproved_repair_reference_is_ignored" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "pending");

    const malformed_body = try repairPrBody(alloc, "agentsfleet-repair/not-valid", "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(malformed_body);
    const malformed = try postSigned(alloc, &s, "pull_request", "del_r11", malformed_body);
    defer malformed.deinit();
    try malformed.expectStatus(.ok);
    try std.testing.expect(malformed.bodyContains("invalid_repair_reference"));

    const unknown_body = try repairPrBody(alloc, &UNKNOWN_REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(unknown_body);
    const unknown = try postSigned(alloc, &s, "pull_request", "del_unknown", unknown_body);
    defer unknown.deinit();
    try unknown.expectStatus(.ok);
    try std.testing.expect(unknown.bodyContains("repair_provenance_refused"));

    const pending_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(pending_body);
    const pending = try postSigned(alloc, &s, "pull_request", "del_r12", pending_body);
    defer pending.deinit();
    try pending.expectStatus(.ok);
    try std.testing.expect(pending.bodyContains("repair_provenance_refused"));
    try std.testing.expectEqual(@as(i64, 0), try repairLinkCount(&s));
}

test "test_foreign_repair_pull_request_is_refused" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "approved");

    const attacker_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "attacker", false, false, "");
    defer alloc.free(attacker_body);
    const attacker = try postSigned(alloc, &s, "pull_request", "del_r13", attacker_body);
    defer attacker.deinit();
    try attacker.expectStatus(.ok);
    try std.testing.expect(attacker.bodyContains("repair_provenance_refused"));

    const fork_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", true, false, "");
    defer alloc.free(fork_body);
    const fork = try postSigned(alloc, &s, "pull_request", "del_r14", fork_body);
    defer fork.deinit();
    try fork.expectStatus(.ok);
    try std.testing.expect(fork.bodyContains("repair_provenance_refused"));
}

test "test_repair_evidence_rows_are_immutable" {
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    try seedRepairAuthority(&s, "approved");

    const pr_body = try repairPrBody(alloc, &REPAIR_BRANCH, "opened", "agentsfleet[bot]", false, false, "");
    defer alloc.free(pr_body);
    const opened = try postSigned(alloc, &s, "pull_request", "del_r15", pr_body);
    defer opened.deinit();
    try opened.expectStatus(.ok);
    const run_body = try repairRunBody(alloc, &REPAIR_BRANCH, 46, "test", REPAIR_MERGE_SHA, "success");
    defer alloc.free(run_body);
    const run = try postSigned(alloc, &s, "workflow_run", "del_r16", run_body);
    defer run.deinit();
    try run.expectStatus(.ok);

    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    try std.testing.expectError(error.PG, conn.exec(
        "UPDATE core.repair_pr_links SET pr_number = 99 WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
    try std.testing.expectError(error.PG, conn.exec(
        "DELETE FROM core.repair_pr_links WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
    try std.testing.expectError(error.PG, conn.exec(
        "UPDATE core.repair_run_results SET conclusion = 'failure' WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
    try std.testing.expectError(error.PG, conn.exec(
        "DELETE FROM core.repair_run_results WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
}
