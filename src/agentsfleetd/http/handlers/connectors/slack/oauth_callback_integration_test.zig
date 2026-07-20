// Integration test — §1 Dim 1.1: the Slack OAuth callback persists a
// connector_installs row and vaults the bot token as the fleet:slack handle,
// with the code-exchange redirected to a loopback fake-Slack.
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.
//
// This is the first connector-integration test (M102's GitHub connector
// shipped without one). It drives the real /v1/connectors/slack/callback route
// through TestHarness: a valid signed state is minted, the fake-Slack answers
// oauth.v2.access with a canned token, and the assertions prove the token lands
// in the vault (RULE VLT) and NOT in the connector_installs table.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const test_port = @import("../../../test_port.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const test_fixtures = @import("../../../../db/test_fixtures.zig");
const vault = @import("../../../../state/vault.zig");
const ec = @import("../../../../errors/error_registry.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const TestHarness = harness_mod.TestHarness;
const net = std.Io.net;
const testing = std.testing;

// UUIDv7-shaped fixtures (version nibble '7' at position 15). Distinct from
// other suites' ids so the shared test DB stays collision-free under the
// parallel runner.
const TENANT_ID = "0195c106-0000-7000-8000-f00000000001"; // per-suite tenant — keeps this suite's workspaces off the shared tenant's FK chain
const TENANT_NAME = "slack-oauth-callback-suite";
const ADMIN_WS = "0195c106-0001-7000-8000-000000000001";
const TARGET_WS = "0195c106-0002-7000-8000-000000000002";
const SIGNING_SECRET = "m106-connect-signing-secret-key!";

// Canned Slack `oauth.v2.access` success body (the shape parseSlackToken
// consumes). team.id is the external_account_id; access_token is the bot token.
const TEAM_ID = "T106TEST";
const BOT_TOKEN = "xoxb-m106-test-tok";
const BOT_USER_ID = "UBOT106";
const INSTALLER_ID = "UADMIN106";
const SLACK_OK_BODY =
    "{\"ok\":true,\"access_token\":\"" ++ BOT_TOKEN ++ "\",\"bot_user_id\":\"" ++ BOT_USER_ID ++ "\"," ++
    "\"scope\":\"app_mentions:read,chat:write,channels:history\"," ++
    "\"team\":{\"id\":\"" ++ TEAM_ID ++ "\",\"name\":\"Acme M106\"}," ++
    "\"authed_user\":{\"id\":\"" ++ INSTALLER_ID ++ "\"}}";

// ── Loopback fake-Slack (mirrors queue/redis_test.zig's PingFake) ────────────
// Each accepted connection drains the request, answers 200 with the canned
// JSON, and closes. Detached per-connection handler so the exchange never
// blocks against a single-threaded accept loop.
const FakeSlack = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *FakeSlack) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *FakeSlack) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        // Wake the parked accept() by dialing ourselves; ignore failure.
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn acceptLoop(self: *FakeSlack) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            const t = std.Thread.spawn(.{}, handleConn, .{stream}) catch {
                stream.close(io);
                continue;
            };
            t.detach();
        }
    }

    fn handleConn(stream: net.Stream) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var sreader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var swriter = stream.writer(io, &write_buf);
        // std.http.Server handles the framing: it parses the request head,
        // drains the form body, sets content-length, and flushes — none of
        // which a hand-rolled socket write got right for std.http.Client.
        var http_server = std.http.Server.init(&sreader.interface, &swriter.interface);
        var req = http_server.receiveHead() catch return;
        req.respond(SLACK_OK_BODY, .{
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = "application/json" }},
        }) catch return;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedSlackAppCreds(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "client_id", .{ .string = "test-client-id" });
    try obj.put(alloc, "client_secret", .{ .string = "test-client-secret" });
    // Key "slack-app": oauth2.loadAppCreds resolves `<provider>-app`.
    try test_fixtures.storeVaultJson(alloc, conn, ADMIN_WS, "slack-app", .{ .object = obj });
}

// Deterministic slate regardless of a prior (possibly failed) run — the install
// upsert + vault store both key on ids we own here.
fn preClean(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2",
        .{ spec.PROVIDER, TEAM_ID },
    ) catch |e| std.log.warn("preclean connector_installs ignored: {s}", .{@errorName(e)});
    _ = vault.deleteCredential(conn, TARGET_WS, spec.PROVIDER) catch |e| std.log.warn("preclean vault ignored: {s}", .{@errorName(e)});
}

test "integration: slack oauth callback persists install + vaults token (Dim 1.1)" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    // Vault crypto needs the process-global KEK (mirrors serve.zig's boot
    // setKekFromHex) — both our creds seed and the handler's storeHandle wrap
    // DEKs, and the readback unwraps them.
    test_fixtures.setTestEncryptionKey();

    // Fixtures: shared tenant + the admin (creds) and target (install) workspaces.
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    preClean(conn);
    try seedSlackAppCreds(alloc, conn);

    // Loopback fake-Slack for the code-exchange.
    var fake: FakeSlack = undefined;
    try fake.start();
    defer fake.shutdown();
    const token_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/api/oauth.v2.access", .{fake.port});
    defer alloc.free(token_url);

    // Wire the Option-C ctx seams before the request (test_harness.zig convention).
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    h.ctx.connector_oauth_token_endpoint_override = token_url;
    h.ctx.app_url = "http://127.0.0.1/";

    // Mint the signed single-use state the callback consumes (binds TARGET_WS).
    const state = try oauth2.mintState(alloc, &h.queue, spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);

    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?code=fake-code&state={s}", .{state});
    defer alloc.free(path);

    // `.unhandled` returns the 302 as-is (else the client chases Location to
    // app_url :80). The install side effects run before the redirect.
    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.found);
    const expected_redirect = "http://127.0.0.1/w/" ++ TARGET_WS ++ "/integrations";
    try testing.expectEqualStrings(expected_redirect, r.header("location") orelse return error.RedirectLocationMissing);

    // (1) Exactly one connector_installs row, mapping team_id → TARGET_WS.
    var q = PgQuery.from(try conn.query(
        "SELECT workspace_id::text, installed_by, array_length(scopes, 1) FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2",
        .{ spec.PROVIDER, TEAM_ID },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.InstallRowMissing;
    const got_ws = (try row.get(?[]const u8, 0)) orelse return error.WorkspaceNull;
    const got_installer = (try row.get(?[]const u8, 1)) orelse return error.InstallerNull;
    const scope_count = (try row.get(?i32, 2)) orelse 0;
    try testing.expectEqualStrings(TARGET_WS, got_ws);
    try testing.expectEqualStrings(INSTALLER_ID, got_installer);
    try testing.expectEqual(@as(i32, 3), scope_count);
    try testing.expect((try q.next()) == null); // unique (provider, external_account_id)

    // (2) The bot token lives ONLY in the (TARGET_WS, slack) vault handle.
    var parsed = try vault.loadJson(alloc, conn, TARGET_WS, spec.PROVIDER);
    defer parsed.deinit();
    const handle = parsed.value.object;
    try testing.expectEqualStrings(BOT_TOKEN, handle.get("bot_token").?.string);
    try testing.expectEqualStrings(BOT_USER_ID, handle.get("bot_user_id").?.string);
    try testing.expectEqualStrings(TEAM_ID, handle.get("team_id").?.string);
}

test "integration: slack oauth callback rejects a forged state (Dim 1.2)" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    preClean(conn);
    try seedSlackAppCreds(alloc, conn);

    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    // Mint a valid single-use state for TARGET_WS, then tamper one byte so the
    // HMAC no longer verifies — a signature-forged state (the security property
    // Dim 1.2 pins), rejected before any code exchange or write.
    const good = try oauth2.mintState(alloc, &h.queue, spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(good);
    const forged = try alloc.dupe(u8, good);
    defer alloc.free(forged);
    forged[forged.len - 1] = if (forged[forged.len - 1] == 'A') 'B' else 'A';

    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/slack/callback?code=whatever&state={s}", .{forged});
    defer alloc.free(path);
    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();

    // Rejected with the GENERIC connector-state-invalid code, consistent with
    // GitHub — NOT a Slack-specific state code (the §1 reconciliation
    // deliberately reused the generic connector code, not a new SLK one).
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_CONNECTOR_STATE_INVALID);

    // No install row was written… (scoped so the result drains — releasing the
    // conn — before the next query on the same connection, else ConnectionBusy).
    {
        var q = PgQuery.from(try conn.query(
            "SELECT count(*) FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2",
            .{ spec.PROVIDER, TEAM_ID },
        ));
        defer q.deinit();
        const row = try q.next() orelse return error.CountRowMissing;
        try testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }

    // …and no slack vault handle was stored for TARGET_WS.
    {
        var vq = PgQuery.from(try conn.query(
            "SELECT count(*) FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2",
            .{ TARGET_WS, spec.PROVIDER },
        ));
        defer vq.deinit();
        const vrow = try vq.next() orelse return error.CountRowMissing;
        try testing.expectEqual(@as(i64, 0), try vrow.get(i64, 0));
    }
}
