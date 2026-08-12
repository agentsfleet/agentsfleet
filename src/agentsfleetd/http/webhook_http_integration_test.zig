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
const db = @import("../db/pool.zig");
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
var wired_webhook_sig: webhook_sig.WebhookSig(*db.Pool) = undefined;
// SAFETY: test fixture; field is populated by the surrounding builder before any read.
var wired_svix: svix_signature.SvixSignature(*db.Pool) = undefined;

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
    const metrics_counters = @import("../observability/metrics_counters.zig");
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "paused") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    const triggered_before = metrics_counters.snapshot().fleet_triggered_total;
    const r = try postSigned(alloc, &s, "workflow_run", "del_a6", FAILURE_BODY);
    defer r.deinit();
    // 200-ignored (not 4xx) so sender retry queues stay quiet for
    // an intentionally paused fleet; nothing accepted → metric unchanged.
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"ignored\":\"fleet_paused\""));
    try std.testing.expectEqual(triggered_before, metrics_counters.snapshot().fleet_triggered_total);
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
    const metrics_counters = @import("../observability/metrics_counters.zig");
    const alloc = std.testing.allocator;
    var s = linearSetup(alloc, "paused") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    cleanupLinearRedis(s.h, alloc);
    defer cleanupLinearRedis(s.h, alloc);

    const triggered_before = metrics_counters.snapshot().fleet_triggered_total;
    const r1 = try postSignedLinear(alloc, &s, LINEAR_BODY);
    defer r1.deinit();
    // 200-ignored (not 4xx): sender retry queues add no value for an
    // intentionally paused fleet; nothing accepted → trigger metric unchanged.
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("\"ignored\":\"fleet_paused\""));
    try std.testing.expectEqual(triggered_before, metrics_counters.snapshot().fleet_triggered_total);

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

const common_c = @import("common");
const repair_links = @import("../state/repair_pr_links.zig");

const REPAIR_INCIDENT_EVENT = "evt-incident-77";
const REPAIR_BRANCH = common_c.REPAIR_BRANCH_PREFIX ++ REPAIR_INCIDENT_EVENT;
const REPAIR_PR_BODY = "{\"action\":\"opened\",\"number\":88,\"repository\":{\"full_name\":\"o/r\"}," ++
    "\"pull_request\":{\"number\":88,\"title\":\"repair\",\"html_url\":\"https://github.com/o/r/pull/88\"," ++
    "\"state\":\"open\",\"draft\":true,\"user\":{\"login\":\"agentsfleet\"}," ++
    "\"head\":{\"ref\":\"" ++ REPAIR_BRANCH ++ "\",\"sha\":\"abc123\"},\"base\":{\"ref\":\"main\"}}}";
const REPAIR_RUN_OK_BODY = "{\"action\":\"completed\",\"workflow_run\":{\"id\":43,\"conclusion\":\"success\"," ++
    "\"run_attempt\":1,\"head_branch\":\"" ++ REPAIR_BRANCH ++ "\"},\"repository\":{\"full_name\":\"o/r\"}}";
const REPAIR_RUN_FAIL_BODY = "{\"action\":\"completed\",\"workflow_run\":{\"id\":44,\"conclusion\":\"failure\"," ++
    "\"run_attempt\":1,\"head_branch\":\"" ++ REPAIR_BRANCH ++ "\"},\"repository\":{\"full_name\":\"o/r\"}}";
const REPAIR_RUN_UNLINKED_BODY = "{\"action\":\"completed\",\"workflow_run\":{\"id\":45,\"conclusion\":\"success\"," ++
    "\"run_attempt\":1,\"head_branch\":\"" ++ common_c.REPAIR_BRANCH_PREFIX ++ "evt-nobody\"},\"repository\":{\"full_name\":\"o/r\"}}";
/// The linked branch name, delivered from a DIFFERENT repository — branch names
/// are not unique across repositories, and this one must not reach the stamp.
const REPAIR_RUN_OTHER_REPO_BODY = "{\"action\":\"completed\",\"workflow_run\":{\"id\":46,\"conclusion\":\"failure\"," ++
    "\"run_attempt\":1,\"head_branch\":\"" ++ REPAIR_BRANCH ++ "\"},\"repository\":{\"full_name\":\"o/elsewhere\"}}";
/// The repository every linked fixture above belongs to.
const REPAIR_REPO = "o/r";

/// Assert the one linkage row for this incident. Row slices live only until the
/// query is released, so the assertions happen inside its scope rather than
/// duplicating the row out to the caller.
fn expectLink(s: *Setup, want: struct {
    repository: []const u8,
    pr_number: i64,
    deploy_status: []const u8,
    stamped: bool,
}) !void {
    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT repository, pr_number, deploy_status, deploy_stamped_at
        \\FROM core.repair_pr_links
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ s.fx.fleet_id, REPAIR_INCIDENT_EVENT }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(want.repository, try row.get([]const u8, 0));
    try std.testing.expectEqual(want.pr_number, try row.get(i64, 1));
    try std.testing.expectEqualStrings(want.deploy_status, try row.get([]const u8, 2));
    try std.testing.expectEqual(want.stamped, (try row.get(?i64, 3)) != null);
}

/// Remove this fixture fleet's linkage rows through the sanctioned purge
/// switch — the same transaction-scoped setting the hard-purge cascade uses.
fn purgeRepairLinks(s: *Setup) void {
    const conn = s.h.acquireConn() catch return;
    defer s.h.releaseConn(conn);
    _ = conn.exec("BEGIN", .{}) catch return;
    _ = conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{}) catch |err|
        std.log.warn("repair link purge ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.repair_pr_links WHERE fleet_id = $1::uuid", .{s.fx.fleet_id}) catch |err|
        std.log.warn("repair link purge ignored: {s}", .{@errorName(err)});
    _ = conn.exec("COMMIT", .{}) catch |err|
        std.log.warn("repair link purge ignored: {s}", .{@errorName(err)});
}

test "test_pr_opened_arm_inserts_once" {
    // Dimension 4.3 — the crew's own draft PR echoing back through the webhook
    // becomes the incident → PR linkage row, NOT a fleet event; a second PR
    // for the same incident is named a duplicate and not recorded.
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    purgeRepairLinks(&s);
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r1", "del_r2" });
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r1", "del_r2" });

    const before = try xlen(s.h, alloc, s.fx.fleet_id);
    const r1 = try postSigned(alloc, &s, "pull_request", "del_r1", REPAIR_PR_BODY);
    defer r1.deinit();
    try r1.expectStatus(.ok);
    try std.testing.expect(r1.bodyContains("linked"));
    // A linkage, not an incident: the event stream did not grow.
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));

    try expectLink(&s, .{
        .repository = REPAIR_REPO,
        .pr_number = 88,
        .deploy_status = repair_links.DEPLOY_STATUS_PENDING,
        .stamped = false,
    });

    // Replay under a fresh delivery id: the row already exists → duplicate,
    // still exactly one linkage, still no event.
    const r2 = try postSigned(alloc, &s, "pull_request", "del_r2", REPAIR_PR_BODY);
    defer r2.deinit();
    try r2.expectStatus(.ok);
    try std.testing.expect(r2.bodyContains("duplicate_repair_link"));
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));
}

test "test_deploy_stamp_and_unknown_branch_noop" {
    // Dimension 4.4 — a completed workflow run on a linked repair branch
    // stamps the deploy result and does NOT wake the fleet (a FAILED run on
    // the crew's own branch is a stamp, not a fresh incident); an unlinked
    // repair-prefixed branch acknowledges and records nothing.
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    requireRedis(s.h) catch return error.SkipZigTest;
    purgeRepairLinks(&s);
    cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r3", "del_r4", "del_r5", "del_r6", "del_r7" });
    defer cleanupRedis(s.h, alloc, s.fx.fleet_id, &.{ "del_r3", "del_r4", "del_r5", "del_r6", "del_r7" });

    // Seed the linkage through the arm itself.
    const seed = try postSigned(alloc, &s, "pull_request", "del_r3", REPAIR_PR_BODY);
    defer seed.deinit();
    try seed.expectStatus(.ok);

    const before = try xlen(s.h, alloc, s.fx.fleet_id);

    // A FAILED completed run on the repair branch: stamped failed, no event —
    // this is the arm's load-bearing half, because a failed workflow_run on
    // any other branch is exactly what wakes this fleet.
    const rf = try postSigned(alloc, &s, "workflow_run", "del_r4", REPAIR_RUN_FAIL_BODY);
    defer rf.deinit();
    try rf.expectStatus(.ok);
    try std.testing.expect(rf.bodyContains(repair_links.DEPLOY_STATUS_FAILED));
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));

    // A later success overwrites the stamp (absolute UPDATE, idempotent).
    const ro = try postSigned(alloc, &s, "workflow_run", "del_r5", REPAIR_RUN_OK_BODY);
    defer ro.deinit();
    try ro.expectStatus(.ok);
    try std.testing.expect(ro.bodyContains(repair_links.DEPLOY_STATUS_OK));
    try expectLink(&s, .{
        .repository = REPAIR_REPO,
        .pr_number = 88,
        .deploy_status = repair_links.DEPLOY_STATUS_OK,
        .stamped = true,
    });

    // The SAME branch name from a DIFFERENT repository must not reach the row.
    // Branch names collide across repositories, so without the repository in
    // the stamp predicate any repo that delivers here could overwrite this
    // incident's outcome — the column an operator reads as "did the fix work".
    const rx = try postSigned(alloc, &s, "workflow_run", "del_r7", REPAIR_RUN_OTHER_REPO_BODY);
    defer rx.deinit();
    try rx.expectStatus(.ok);
    try std.testing.expect(rx.bodyContains("unlinked_repair_branch"));
    try expectLink(&s, .{
        .repository = REPAIR_REPO,
        .pr_number = 88,
        .deploy_status = repair_links.DEPLOY_STATUS_OK,
        .stamped = true,
    });

    // Unknown repair-prefixed branch: acknowledged, nothing recorded, no event.
    const ru = try postSigned(alloc, &s, "workflow_run", "del_r6", REPAIR_RUN_UNLINKED_BODY);
    defer ru.deinit();
    try ru.expectStatus(.ok);
    try std.testing.expect(ru.bodyContains("unlinked_repair_branch"));
    try std.testing.expectEqual(before, try xlen(s.h, alloc, s.fx.fleet_id));
}

test "test_repair_link_store_immutability" {
    // Dimension 4.2 — the schema trigger, not store discipline, is what holds
    // content immutable: a content UPDATE and a bare DELETE both raise; the
    // deploy stamp is the single permitted mutation.
    const alloc = std.testing.allocator;
    var s = Setup.init(alloc, "active") catch |err| return skipOrErr(err);
    defer s.deinit(alloc);
    purgeRepairLinks(&s);

    const conn = try s.h.acquireConn();
    defer s.h.releaseConn(conn);
    const outcome = try repair_links.insert(alloc, conn, .{
        .workspace_id = s.fx.workspace_id,
        .fleet_id = s.fx.fleet_id,
        .event_id = REPAIR_INCIDENT_EVENT,
        .repository = REPAIR_REPO,
        .branch = REPAIR_BRANCH,
        .pr_number = 88,
        .pr_url = "https://github.com/o/r/pull/88",
    });
    try std.testing.expectEqual(repair_links.InsertOutcome.inserted, outcome);

    // Insert-only on the incident key: a second insert is a duplicate.
    const again = try repair_links.insert(alloc, conn, .{
        .workspace_id = s.fx.workspace_id,
        .fleet_id = s.fx.fleet_id,
        .event_id = REPAIR_INCIDENT_EVENT,
        .repository = "o/other",
        .branch = REPAIR_BRANCH,
        .pr_number = 89,
        .pr_url = "https://github.com/o/r/pull/89",
    });
    try std.testing.expectEqual(repair_links.InsertOutcome.duplicate, again);

    // Content is frozen by trigger. Autocommit: each refused statement is its
    // own aborted transaction, so the conn stays usable for the next one.
    try std.testing.expectError(error.PG, conn.exec(
        "UPDATE core.repair_pr_links SET pr_number = 99 WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));
    try std.testing.expectError(error.PG, conn.exec(
        "DELETE FROM core.repair_pr_links WHERE fleet_id = $1::uuid",
        .{s.fx.fleet_id},
    ));

    // The one permitted mutation still works after the refusals — and only for
    // the repository the row actually names.
    try std.testing.expect(!try repair_links.stampDeploy(conn, s.fx.fleet_id, "o/elsewhere", REPAIR_BRANCH, repair_links.DEPLOY_STATUS_FAILED));
    try std.testing.expect(try repair_links.stampDeploy(conn, s.fx.fleet_id, REPAIR_REPO, REPAIR_BRANCH, repair_links.DEPLOY_STATUS_OK));
}
