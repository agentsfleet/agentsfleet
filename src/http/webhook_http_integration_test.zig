// End-to-end HTTP integration tests for per-zombie webhook auth.
//
// Uses the shared TestHarness (src/http/test_harness.zig) with the real
// webhook_sig + svix_signature middlewares wired to the production
// serve_webhook_lookup callbacks — so a 202 proves the full path:
//   router → middleware → vault lookup → handler → redis dedup → 202.
//
// LIVE DB ONLY. Requires `make test-integration` (or `make up`, then
// TEST_DATABASE_URL set + LIVE_DB=1). Tests skip when DB is not reachable.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const webhook_sig = @import("../auth/middleware/webhook_sig.zig");
const svix_signature = @import("../auth/middleware/svix_signature.zig");
const serve_webhook_lookup = @import("../cmd/serve_webhook_lookup.zig");

const harness_mod = @import("test_harness.zig");
const fx_mod = @import("webhook_test_fixtures.zig");
const signers = @import("webhook_test_signers.zig");

const TestHarness = harness_mod.TestHarness;

// ── Middleware wiring ─────────────────────────────────────────────────────
//
// The harness passes `*TestHarness` into configureRegistry; we construct
// WebhookSig + SvixSignature pinned to the harness's pool and register them.
// The middleware instances live at module scope so their addresses stay stable
// across the test body (chain.Middleware holds raw pointers into them).
//
// `zig build test` runs tests sequentially within a single process, so
// reassigning across tests is safe. If the runner ever parallelizes, these
// need to move into TestHarness as a lifetime-managed extension.

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

// ── §1.1: Scaffold sanity — server starts, /healthz returns 200 ──────────────

test "integration: webhook harness — healthz reachable" {
    const alloc = std.testing.allocator;
    const h = startHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get("/healthz").send();
    defer r.deinit();
    try r.expectStatus(.ok);
}

// ── §2.1: GitHub happy path — HMAC-SHA256 over body, sha256= prefix ─────────
//
// TRACKED SKIP: handler's dedupAndEnqueue path calls Redis (setNx + xadd) via
// ctx.queue, which TestHarness currently leaves as `undefined`. Finishing
// this test requires extending TestHarness.Config with an optional Redis
// wiring (connect from TEST_REDIS_TLS_URL + REDIS_TLS_CA_CERT_FILE set by
// `make test-integration`). Until then the test would SIGKILL in the
// handler's Redis call. Suites B–F tests that assert pre-handler rejections
// (401 from middleware) do not need Redis and will land without this
// extension. Follow-up workstream: harness Redis wiring.

test "integration: github webhook — valid signature yields 202" {
    return error.SkipZigTest; // TRACKED — see block comment above
}

// Compile-only anchor. Zig analyzes functions lazily; an unreferenced private
// fn is never type-checked. Calling _tracked_github_happy_path from a test
// (guarded by an immediate skip) keeps it on the reachable-from-entry-point
// graph, so refactors to TestHarness / fixtures / signers surface as compile
// errors here instead of silently rotting the tracked body.
test "comptime: tracked github happy path compiles" {
    if (true) return error.SkipZigTest;
    try _tracked_github_happy_path(std.testing.allocator);
}

fn _tracked_github_happy_path(alloc: std.mem.Allocator) !void {
    const h = startHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const fx: fx_mod.Fixture = .{
        .tenant_id = fx_mod.ID_TENANT_A,
        .workspace_id = fx_mod.ID_WS_A,
        .zombie_id = fx_mod.ID_ZOMBIE_A,
    };
    const secret_plaintext = "topsecret-github-key";

    const trigger_json = try fx_mod.buildTriggerConfig(alloc, "github", null);
    defer alloc.free(trigger_json);

    // ── Insert fixture rows ───────────────────────────────────────────────
    const conn = try h.acquireConn();
    try fx_mod.insertZombie(conn, fx, trigger_json);
    try fx_mod.insertWebhookCredential(alloc, conn, fx.workspace_id, "github", secret_plaintext);
    h.releaseConn(conn);

    // ── Sign + POST the webhook payload ───────────────────────────────────
    const body =
        \\{"event_id":"evt_gh_001","type":"issues.opened","data":{"action":"opened"}}
    ;
    const sig = try signers.signGithub(alloc, secret_plaintext, body);
    defer sig.deinit(alloc);

    const url = try std.fmt.allocPrint(alloc, "/v1/webhooks/{s}", .{fx.zombie_id});
    defer alloc.free(url);

    const r = try (try (try h.post(url).header(sig.header_name, sig.header_value)).json(body)).send();
    defer r.deinit();

    // ── Assert + clean up ─────────────────────────────────────────────────
    // Cleanup runs BEFORE expectStatus so a failed assertion doesn't leave
    // orphaned rows; acceptable because cleanup is idempotent.
    const cleanup_conn = try h.acquireConn();
    try fx_mod.cleanup(cleanup_conn, fx);
    h.releaseConn(cleanup_conn);

    try r.expectStatus(.accepted);
    try std.testing.expect(r.bodyContains("accepted") or r.bodyContains("evt_gh_001"));
}
