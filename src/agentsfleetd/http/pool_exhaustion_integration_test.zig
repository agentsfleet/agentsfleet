//! Pool-acquire arms: what a handler answers when the database pool has no
//! connection left to give.
//!
//! Every one of these arms is the code an operator meets during an incident —
//! the moment the daemon is busiest and least observed. None of them had ever
//! run: an ordinary integration test always finds a free connection, so the
//! `catch` beside each `pool.acquire()` is dead weight until something starves
//! the pool. The starvation is induced here rather than simulated, so what is
//! asserted is the real arm on the real handler.
//!
//! The pool's acquire timeout (2s by default) bounds each request, so a test
//! here costs seconds, not milliseconds. That is the price of driving the arm
//! through the handler instead of calling it directly, and calling it directly
//! would prove only that it compiles. One drain serves the whole table: the
//! pool is starved once and every endpoint probed against it.
//!
//! Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise.

const std = @import("std");
const pg = @import("pg");

const base = @import("secrets_json_integration_test.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");
const scope_tokens = @import("test_scope_tokens.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

/// Well-formed UUIDv7s that name no row. Every arm below sits BEHIND id-shape
/// validation and AHEAD of the lookup, so an id must parse to reach the arm
/// while never needing to exist. A seeded id would make the row depend on
/// fixture state the arm has nothing to do with.
const ABSENT_KEY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af001";
const ABSENT_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af002";
const ABSENT_EVENT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af003";
const ABSENT_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af004";
const ABSENT_GRANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af005";
const ABSENT_GATE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af006";
const ABSENT_MODEL_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af007";

// Composed at comptime from the seeded fixture ids, so a row is one line and
// no probe has to allocate a path.
const WS = "/v1/workspaces/" ++ base.TEST_WS_ID;
const FLEET = WS ++ "/fleets/" ++ ABSENT_FLEET_ID;
const RUNNER = "/v1/fleets/runners/" ++ ABSENT_RUNNER_ID;

const ADMIN = base.TOKEN_OPERATOR;
// Imported straight from the persona fixtures: the platform plane needs
// runner/model/platform-key scopes the tenant-admin persona does not carry,
// and re-exporting it through the seed module would widen that module for
// one caller.
const PLATFORM = scope_tokens.PLATFORM_ADMIN;

/// One starved endpoint and the handler whose acquire arm it reaches.
///
/// `owner` is what stops the table being padding: each row names a distinct
/// arm, so a row that duplicates its neighbour's coverage is visible on sight
/// rather than hidden behind an identical 503.
const Probe = struct {
    method: std.http.Method,
    path: []const u8,
    token: []const u8,
    owner: []const u8,
    /// Only set where the verb requires one — see the note on the table.
    body: ?[]const u8 = null,
};

/// Bodiless verbs, plus the two bodied rows whose bodies are already known to
/// clear validation. The harness cannot send a bodiless PUT/POST, and a guessed
/// body that fails validation lands on the rejection arm instead of the acquire
/// — so the remaining bodied verbs are a separate pass with bodies read from
/// each handler's validator, not a guess bolted onto this table.
const PROBES = [_]Probe{
    .{ .method = .GET, .path = "/v1/api-keys", .token = ADMIN, .owner = "api_keys/list.innerListApiKeys" },
    .{ .method = .GET, .path = "/v1/tenants/me/billing", .token = ADMIN, .owner = "tenant_billing.innerGetTenantBilling" },
    .{ .method = .GET, .path = "/v1/tenants/me/billing/charges", .token = ADMIN, .owner = "tenant_billing.innerGetTenantBillingCharges" },
    .{ .method = .GET, .path = "/v1/tenants/me/workspaces", .token = ADMIN, .owner = "tenant_workspaces.innerListTenantWorkspaces" },
    .{ .method = .GET, .path = "/v1/tenants/me/provider", .token = ADMIN, .owner = "tenant_provider.innerGetTenantProvider" },
    .{ .method = .DELETE, .path = "/v1/tenants/me/provider", .token = ADMIN, .owner = "tenant_provider.innerDeleteTenantProvider" },
    .{ .method = .DELETE, .path = "/v1/tenants/me/models/" ++ ABSENT_MODEL_ID, .token = ADMIN, .owner = "tenant_model_entries_delete.innerDeleteModelEntry" },

    .{ .method = .GET, .path = WS ++ "/onboarding", .token = ADMIN, .owner = "workspaces/onboarding.innerGetOnboarding" },
    .{ .method = .GET, .path = WS ++ "/events", .token = ADMIN, .owner = "workspaces/events.innerListWorkspaceEvents" },
    .{ .method = .GET, .path = WS ++ "/approvals", .token = ADMIN, .owner = "approvals/list.innerListApprovals" },
    .{ .method = .GET, .path = WS ++ "/approvals/" ++ ABSENT_GATE_ID, .token = ADMIN, .owner = "approvals/detail.innerGetApproval" },
    .{ .method = .GET, .path = WS ++ "/secrets", .token = ADMIN, .owner = "fleets/secrets.innerListSecrets" },
    .{ .method = .DELETE, .path = WS ++ "/secrets/pool-starved", .token = ADMIN, .owner = "fleets/secrets.innerDeleteSecret" },
    .{ .method = .GET, .path = WS ++ "/connectors", .token = ADMIN, .owner = "connectors/catalog.innerCatalog" },
    .{ .method = .GET, .path = WS ++ "/connectors/slack", .token = ADMIN, .owner = "connectors/status.innerStatus" },
    .{ .method = .DELETE, .path = WS ++ "/connectors/slack", .token = ADMIN, .owner = "connectors/disconnect.innerDisconnect" },

    .{ .method = .GET, .path = WS ++ "/fleets", .token = ADMIN, .owner = "fleets/list.innerListFleets" },
    .{ .method = .GET, .path = FLEET, .token = ADMIN, .owner = "fleets/get.innerGetFleet" },
    .{ .method = .DELETE, .path = FLEET, .token = ADMIN, .owner = "fleets/delete.innerDeleteFleet" },
    .{ .method = .GET, .path = FLEET ++ "/events", .token = ADMIN, .owner = "fleets/events.innerListEvents" },
    .{ .method = .GET, .path = FLEET ++ "/events/" ++ ABSENT_EVENT_ID, .token = ADMIN, .owner = "fleets/event_detail.innerGetEvent" },
    .{ .method = .GET, .path = FLEET ++ "/messages", .token = ADMIN, .owner = "fleets/messages_list.innerListFleetMessages" },
    .{ .method = .GET, .path = FLEET ++ "/memories", .token = ADMIN, .owner = "memory/handler.innerListMemories" },
    .{ .method = .DELETE, .path = FLEET ++ "/memories/pool-starved", .token = ADMIN, .owner = "memory/handler.innerDeleteMemory" },
    .{ .method = .GET, .path = FLEET ++ "/integration-grants", .token = ADMIN, .owner = "integration_grants/workspace.innerListGrants" },
    .{ .method = .DELETE, .path = FLEET ++ "/integration-grants/" ++ ABSENT_GRANT_ID, .token = ADMIN, .owner = "integration_grants/workspace.innerRevokeGrant" },

    .{ .method = .GET, .path = "/v1/admin/models", .token = PLATFORM, .owner = "admin/model_library_admin.innerGetAdminModels" },
    .{ .method = .DELETE, .path = "/v1/admin/models/" ++ ABSENT_MODEL_ID, .token = PLATFORM, .owner = "admin/model_library_admin.innerDeleteAdminModel" },
    .{ .method = .GET, .path = "/v1/admin/platform-keys", .token = PLATFORM, .owner = "admin/platform_keys.innerGetAdminPlatformKeys" },
    .{ .method = .DELETE, .path = "/v1/admin/platform-keys/openai", .token = PLATFORM, .owner = "admin/platform_keys.innerDeleteAdminPlatformKey" },
    .{ .method = .GET, .path = "/v1/fleets/runners", .token = PLATFORM, .owner = "fleet/runners_list.innerListFleetRunners" },
    .{ .method = .GET, .path = RUNNER ++ "/events", .token = PLATFORM, .owner = "fleet/runner_events.innerListFleetRunnerEvents" },
    .{ .method = .DELETE, .path = RUNNER, .token = PLATFORM, .owner = "fleet/runner_delete.innerDeleteFleetRunner" },

    .{ .method = .DELETE, .path = "/v1/api-keys/" ++ ABSENT_KEY_ID, .token = ADMIN, .owner = "api_keys/tenant.innerDeleteApiKey" },
    .{ .method = .POST, .path = "/v1/api-keys", .token = ADMIN, .owner = "api_keys/tenant.innerCreateApiKey", .body = "{\"key_name\":\"pool-starved\"}" },
    .{ .method = .PATCH, .path = "/v1/api-keys/" ++ ABSENT_KEY_ID, .token = ADMIN, .owner = "api_keys/tenant.innerPatchApiKey", .body = "{\"active\":false}" },
};

const Held = std.ArrayListUnmanaged(*pg.Conn);

/// Take every connection the pool will give, so the next acquire cannot be
/// served.
///
/// Size-agnostic on purpose: the pool size is env-tunable, and a hardcoded
/// count would quietly stop exhausting anything the day the default moves —
/// the tests would keep passing while asserting nothing. Draining until the
/// pool refuses costs one acquire timeout, once.
fn drainPool(h: *TestHarness, held: *Held) !void {
    while (true) {
        const conn = h.acquireConn() catch break;
        try held.append(ALLOC, conn);
    }
    // A pool that handed back nothing was already starved by something else,
    // which would make every assertion below vacuously true.
    try std.testing.expect(held.items.len > 0);
}

fn releaseAll(h: *TestHarness, held: *Held) void {
    for (held.items) |conn| h.releaseConn(conn);
    held.deinit(ALLOC);
}

/// Probe every row, reporting ALL mismatches before failing.
///
/// One drain plus one acquire timeout per row is a minute of wall clock, and
/// the integration lane is the only place this can run — so a stop-at-first-
/// failure loop would spend a whole lane to learn about one bad row. Every
/// mismatch is printed with the arm it was aiming at; the count is what fails.
fn probeAll(h: *TestHarness, probes: []const Probe) !void {
    var bad: usize = 0;
    for (probes) |p| {
        var req = h.request(p.method, p.path);
        req = try req.bearer(p.token);
        if (p.body) |b| req = try req.json(b);
        const r = try req.send();
        defer r.deinit();
        const want: u16 = @intFromEnum(std.http.Status.service_unavailable);
        const code_ok = std.mem.indexOf(u8, r.body, ec.ERR_INTERNAL_DB_UNAVAILABLE) != null;
        if (r.status != want or !code_ok) {
            bad += 1;
            std.debug.print(
                "  starved probe MISS: {s} {s} -> {d} (want {d}, {s}) arm={s}\n",
                .{ @tagName(p.method), p.path, r.status, want, ec.ERR_INTERNAL_DB_UNAVAILABLE, p.owner },
            );
        }
    }
    try std.testing.expectEqual(@as(usize, 0), bad);
}

test "integration: test_pool_exhaustion_answers_unavailable — every acquiring handler answers 503" {
    base.setTestEncryptionKey();
    const h = base.seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var held: Held = .empty;
    try drainPool(h, &held);
    defer releaseAll(h, &held);

    try probeAll(h, &PROBES);
}

test "integration: test_pool_recovers_when_connections_return — starvation is not a latch" {
    // The arms above must be a transient answer, not a state the daemon gets
    // stuck in. A pool that never recovers turns one slow query into a
    // permanent outage, and nothing else in the suite would notice: every
    // other test starts from a healthy pool and never starves it.
    base.setTestEncryptionKey();
    const h = base.seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const prefs_path = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/preferences", .{base.TEST_WS_ID});
    defer ALLOC.free(prefs_path);

    var held: Held = .empty;
    // Released explicitly below; this catches the drain failing part-way, where
    // the connections taken so far would otherwise be stranded.
    defer releaseAll(h, &held);
    try drainPool(h, &held);
    {
        const r = try (try h.get(prefs_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
    }

    // Hand every connection back; the very next request must be served.
    releaseAll(h, &held);
    held = .empty;

    // Asserted through the key delete rather than the preference read: a
    // served preference read still needs a `core.users` row this fixture does
    // not seed, so it would answer 403 and prove nothing about the pool. The
    // delete answers 404 for an id that does not exist — an answer only a
    // handler holding a real connection can give.
    const key_path = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{ABSENT_KEY_ID});
    defer ALLOC.free(key_path);
    const r = try (try h.delete(key_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}
