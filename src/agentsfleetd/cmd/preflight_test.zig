const std = @import("std");
const constants = @import("common");
const preflight = @import("preflight.zig");
const cmd_common = @import("common.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const otel_logs = @import("../observability/otel_logs.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const otel_metrics = @import("../observability/otel_metrics.zig");

// `initPostHog` and `parseMigrateOnStart` read an injected `common.env.Map` rather
// than the process environment, so each test builds the exact environment it means.
// Mutating the real environment would leak across tests sharing this process.

// ---------------------------------------------------------------------------
// PostHog init
// ---------------------------------------------------------------------------

test "initPostHog returns null client when POSTHOG_API_KEY is unset" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    const result = preflight.initPostHog(&env, alloc);
    defer result.deinit(alloc);

    try std.testing.expect(result.client == null);
    try std.testing.expect(result.api_key_owned == null);
}

test "PostHogResult deinit is safe when both fields are null" {
    const result = preflight.PostHogResult{
        .client = null,
        .api_key_owned = null,
    };
    result.deinit(std.testing.allocator);
}

test "initPostHog builds a client when the key is present, and deinit owns both" {
    // No event is ever captured, so the flush thread has nothing to send and
    // the network is never touched — construction and teardown are the claim.
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "POSTHOG_API_KEY", "phc_test_key" }});
    defer env.deinit();

    const result = preflight.initPostHog(&env, alloc);
    defer result.deinit(alloc);

    try std.testing.expect(result.client != null);
    try std.testing.expect(result.api_key_owned != null);
}

test "initPostHog disables analytics when the client cannot be built" {
    // The key is present, so the copy succeeds and the function goes on to
    // build a client — which then fails. Two claims: the daemon still boots
    // (a telemetry failure is never a reason to refuse to start), and the arm
    // frees the key it had already copied. `testing.allocator` underneath
    // fails the test on the leak the second claim prevents.
    //
    // Ladder rather than one index: `posthog.init`'s first allocation is an
    // implementation detail, so every index past the key copy is driven until
    // one of them lands on it.
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "POSTHOG_API_KEY", "phc_ladder_key" }});
    defer env.deinit();

    var saw_disabled = false;
    for (1..8) |fail_index| {
        var failing = std.testing.FailingAllocator.init(alloc, .{ .fail_index = fail_index });
        const result = preflight.initPostHog(&env, failing.allocator());
        defer result.deinit(failing.allocator());
        if (result.client == null) {
            // Disabled, and it did NOT keep the key it copied on the way in.
            try std.testing.expect(result.api_key_owned == null);
            saw_disabled = true;
        }
    }
    try std.testing.expect(saw_disabled);
}

test "initTelemetry carries the PostHog client and exposes a stable pointer" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    var result = preflight.initTelemetry(&env, alloc);
    defer result.deinit(alloc);

    // Unconfigured: prod telemetry with no client — the pointer is what the
    // handler layer stores, so it must address this result's own field.
    try std.testing.expectEqual(&result.telemetry, result.ptr());
}

// ---------------------------------------------------------------------------
// Database pool
// ---------------------------------------------------------------------------

test "connectDbPool refuses an environment with no database URL" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    try std.testing.expectError(
        error.MissingDatabaseUrl,
        preflight.connectDbPool(constants.globalIo(), &env, alloc, .api),
    );
}

// ---------------------------------------------------------------------------
// Migration parse
// ---------------------------------------------------------------------------

test "parseMigrateOnStart returns true for '1'" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "1" }});
    defer env.deinit();
    try std.testing.expect(try preflight.parseMigrateOnStart(&env, alloc));
}

test "parseMigrateOnStart returns false for '0'" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "0" }});
    defer env.deinit();
    try std.testing.expect(!try preflight.parseMigrateOnStart(&env, alloc));
}

test "parseMigrateOnStart surfaces a value outside the boolean grammar" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "maybe" }});
    defer env.deinit();
    try std.testing.expectError(cmd_common.MigrationGuardError.InvalidMigrateOnStart, preflight.parseMigrateOnStart(&env, alloc));
}

// ---------------------------------------------------------------------------
// Credential broker handle ownership
// ---------------------------------------------------------------------------

test "CredentialBrokerHandle deinit frees a partially-built install" {
    // The degrade-closed contract: whatever subset of fields an aborted install
    // managed to set, deinit frees exactly that subset. The leak detector on
    // testing.allocator is the assertion.
    const alloc = std.testing.allocator;
    var handle = preflight.CredentialBrokerHandle{
        .alloc = alloc,
        .github_app = .{
            .app_id = try alloc.dupe(u8, "12345"),
            .private_key_pem = try alloc.dupe(u8, "-----BEGIN TEST KEY-----"),
            .app_slug = try alloc.dupe(u8, "agentsfleet-test"),
        },
        .zoho_app = .{
            .client_id = try alloc.dupe(u8, "zoho-client"),
            .client_secret = try alloc.dupe(u8, "zoho-secret-fixture"),
        },
    };
    handle.deinit();
}

test "CredentialBrokerHandle deinit is safe on the nothing-built handle" {
    var handle = preflight.CredentialBrokerHandle{ .alloc = std.testing.allocator };
    handle.deinit();
}

// ---------------------------------------------------------------------------
// Signal handlers
// ---------------------------------------------------------------------------

var test_signal_received = std.atomic.Value(bool).init(false);

fn testSignalHandler(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    test_signal_received.store(true, .release);
}

test "installSignalHandlers routes a delivered INT to the given handler" {
    // The suite runs in this process: leaving our handler installed would swallow
    // a real Ctrl-C for every test that follows, so the previous actions are
    // restored before returning.
    var prev_int: std.posix.Sigaction = undefined;
    var prev_term: std.posix.Sigaction = undefined;
    std.posix.sigaction(std.posix.SIG.INT, null, &prev_int);
    std.posix.sigaction(std.posix.SIG.TERM, null, &prev_term);
    defer {
        std.posix.sigaction(std.posix.SIG.INT, &prev_int, null);
        std.posix.sigaction(std.posix.SIG.TERM, &prev_term, null);
    }
    test_signal_received.store(false, .release);

    preflight.installSignalHandlers(testSignalHandler);

    var installed_int: std.posix.Sigaction = undefined;
    var installed_term: std.posix.Sigaction = undefined;
    std.posix.sigaction(std.posix.SIG.INT, null, &installed_int);
    std.posix.sigaction(std.posix.SIG.TERM, null, &installed_term);

    const expected = @intFromPtr(&testSignalHandler);
    try std.testing.expectEqual(expected, @intFromPtr(installed_int.handler.handler.?));
    try std.testing.expectEqual(expected, @intFromPtr(installed_term.handler.handler.?));

    // Raise only AFTER the handler is proven installed: on the default action a
    // delivered INT would terminate the test runner instead of failing this test.
    try std.posix.raise(std.posix.SIG.INT);
    try std.testing.expect(test_signal_received.load(.acquire));
}

// ---------------------------------------------------------------------------
// OTLP exporter config ownership
// ---------------------------------------------------------------------------

test "initOtelExporters owns the config on the already-running path (no leak)" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{
        .{ "GRAFANA_OTLP_ENDPOINT", "https://otlp.example" },
        .{ "GRAFANA_OTLP_INSTANCE_ID", "12345" },
        .{ "GRAFANA_OTLP_API_KEY", "k" },
    });
    defer env.deinit();

    // Force .already_running on all three installs. The handle must still own
    // and free the freshly parsed config — before the ownership fix this path
    // dropped it on the floor (and spawn_failed nulled it without freeing).
    const static_cfg: otlp_config.GrafanaOtlpConfig = .{ .endpoint = "e", .instance_id = "i", .api_key = "k" };
    otel_logs.testSetInstalled(static_cfg);
    defer otel_logs.testClear();
    otel_traces.testSetInstalled(static_cfg);
    defer otel_traces.testClear();
    otel_metrics.testSetInstalled(static_cfg);
    defer otel_metrics.testClear();

    var handle = preflight.initOtelExporters(constants.globalIo(), &env, alloc);
    handle.deinit(alloc); // testing.allocator's leak detector is the assertion
}

test "initOtelExporters returns an empty handle when unconfigured (deinit safe)" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();
    var handle = preflight.initOtelExporters(constants.globalIo(), &env, alloc);
    handle.deinit(alloc);
}

test "migrateOnStartEnabledFromEnv accepts the shared trimmed grammar (Dimension 4.3)" {
    // Pre-fix this site parsed untrimmed while the dotenv gate trimmed —
    // " true" enabled one boolean and boot-errored the other.
    const alloc = std.testing.allocator;
    var padded = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", " true" }});
    defer padded.deinit();
    try std.testing.expect(try cmd_common.migrateOnStartEnabledFromEnv(&padded, alloc));

    var padded_zero = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", " 0\t" }});
    defer padded_zero.deinit();
    try std.testing.expect(!try cmd_common.migrateOnStartEnabledFromEnv(&padded_zero, alloc));
}

// ---------------------------------------------------------------------------
// The paths below had no executed lines: PostHog WITH a key, the telemetry
// bundle, the OTLP fresh-install path, the pool connect (both verdicts), the
// migration check against a migrated schema, and the credential broker's boot.
// The live-database cases guard on TEST_DATABASE_URL exactly like pool_test —
// skipped in the plain lane, executed in the coverage lane.
// ---------------------------------------------------------------------------

const dts_serve = @import("serve_deadline.zig");
const credential_broker = @import("../credentials/broker.zig");

test "initPostHog builds a client from a present key and tears it down" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "POSTHOG_API_KEY", "phc_test_probe_key" }});
    defer env.deinit();

    const result = preflight.initPostHog(&env, alloc);
    defer result.deinit(alloc);

    // Init is offline (the flush thread dials lazily); a present key must
    // yield a client, or analytics silently vanish for the process lifetime.
    try std.testing.expect(result.client != null);
    try std.testing.expectEqualStrings("phc_test_probe_key", result.api_key_owned.?);
}

test "initTelemetry carries the PostHog outcome and stays deinit-safe" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    var t = preflight.initTelemetry(&env, alloc);
    defer t.deinit(alloc);
    _ = t.ptr(); // the borrowed pointer serve.zig threads through Context
}

test "connectDbPool refuses when the role's URL is unset" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    try std.testing.expectError(
        error.MissingDatabaseUrl,
        preflight.connectDbPool(constants.globalIo(), &env, alloc, .api),
    );
}

/// Environment for the live-database preflight cases: the API role's URL from
/// the test datastore, or null → the caller skips (plain lane).
fn liveDbEnv(alloc: std.mem.Allocator) !?constants.env.Map {
    const url = constants.env.testLiveValue("TEST_DATABASE_URL") orelse return null;
    return try constants.env.fromPairs(alloc, &.{.{ "DATABASE_URL_API", url }});
}

test "connectDbPool reaches the live test database and checkMigrations passes" {
    const alloc = std.testing.allocator;
    var env = (try liveDbEnv(alloc)) orelse return error.SkipZigTest;
    defer env.deinit();
    const io = constants.globalIo();

    const pool = try preflight.connectDbPool(io, &env, alloc, .api);
    defer pool.deinit();

    // The integration bootstrap migrated this database; the guard must read
    // that state as clean rather than demanding MIGRATE_ON_START.
    try preflight.checkMigrations(io, &env, alloc, pool, false);
}

test "installCredentialBroker publishes a live broker and tears down cleanly" {
    const alloc = std.testing.allocator;
    var env = (try liveDbEnv(alloc)) orelse return error.SkipZigTest;
    defer env.deinit();
    const io = constants.globalIo();

    const pool = try preflight.connectDbPool(io, &env, alloc, .api);
    defer pool.deinit();

    var deadlines: dts_serve.Owned = .{};
    defer deadlines.deinit();
    const sched = deadlines.start(alloc);

    var broker_out: ?*credential_broker = null;
    var slug_out: ?[]const u8 = null;
    var handle = preflight.installCredentialBroker(
        alloc,
        io,
        sched,
        pool,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaaaa",
        &broker_out,
        &slug_out,
    );
    defer handle.deinit();

    // Degrades closed on any missing platform key, but the boot itself must
    // publish a broker — a silent null 503s every mint for the process.
    try std.testing.expect(broker_out != null);
}
