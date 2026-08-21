//! Pool starvation on the runner self-plane: the five `pool.acquire()` arms
//! that sit behind `agt_r` auth.
//!
//! Sibling of `pool_exhaustion_integration_test.zig`, which starves the routes a
//! session or platform token reaches. Its probe table records why these five
//! were left open — "the rest take a minted `agt_r` this table has no way to
//! carry yet". The shared seed fixture cannot carry one: that module's
//! `configureRegistry` is a deliberate no-op, so every runner route 401s there,
//! and teaching it the runner lookup would mean seeding a `fleet.runners` row
//! into a fixture eleven suites share, for one caller.
//!
//! ## The lookup is a pool-free stub, on purpose
//!
//! `serve_runner_lookup.lookup` acquires a connection of its own to resolve the
//! token hash, and answers `error.DbUnavailable` when it cannot get one — which
//! `runnerBearer` maps to `UZ-AUTH-004` before any handler runs. Wired against a
//! drained pool it would short-circuit all five routes at the middleware and
//! leave every arm below unexecuted, while still reporting a pass. That
//! middleware arm already carries its own proof in
//! `auth/middleware/runner_bearer.zig` ("maps a lookup failure to UZ-AUTH-004,
//! never an auth reject"), so nothing is lost by keeping it out of this file.
//!
//! Resolving without a connection models what these arms answer in production:
//! auth takes a connection and hands it back, and the pool is empty by the time
//! the handler acquires. Under contention that is an ordinary interleaving. A
//! total outage never reaches these arms — it stops at the middleware.

const std = @import("std");

const auth_mw = @import("../auth/middleware/mod.zig");
const harness_mod = @import("test_harness.zig");
const integration = @import("../credentials/integration.zig");
const CredentialBroker = @import("../credentials/broker.zig");
const starve = @import("pool_exhaustion_integration_test.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

/// Well-formed UUIDv7s naming no row. Every arm below sits behind id-shape
/// validation and ahead of the lookup, so an id must parse while never needing
/// to exist.
const ABSENT_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af101";
const ABSENT_LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af102";

/// The principal the stub mints. No handler below gets far enough to read a row
/// for it — the starved acquire returns first.
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af103";

const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "f" ** 64;

const ME = "/v1/runners/me";
const MEMORY = ME ++ "/memory/" ++ ABSENT_FLEET_ID;

/// Resolve any well-formed `agt_r` to an active, non-degraded runner without
/// touching the pool — see the note at the top of the file.
fn poolFreeRunnerLookup(
    _: *anyopaque,
    alloc: std.mem.Allocator,
    _: []const u8,
) anyerror!?auth_mw.runner_bearer.LookupResult {
    return .{
        .runner_id = try alloc.dupe(u8, RUNNER_ID),
        .active = true,
        // Defaults to true (fail closed), which the lease gate refuses ahead of
        // the arms this file aims at.
        .degraded = false,
    };
}

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {
    // SAFETY: poolFreeRunnerLookup ignores host, so .host is never dereferenced
    // — the same guarantee the harness default declares for its null stub.
    reg.runner_bearer_mw = .{ .host = undefined, .lookup = poolFreeRunnerLookup };
}

/// One row per runner-plane acquire arm. Bodies are read from each handler's
/// validator, not guessed: a body that fails validation lands on the rejection
/// arm and colours the wrong line.
const PROBES = [_]starve.Probe{
    .{ .method = .GET, .path = ME, .token = RUNNER_TOKEN, .owner = "runner/self.innerRunnerSelf" },
    // Any small body reaches the acquire: `parseCapabilityReport` answers
    // `.none` for an absent, empty, unparseable or out-of-bounds report, and
    // short-circuits only on an over-size one.
    .{ .method = .POST, .path = ME ++ "/heartbeats", .token = RUNNER_TOKEN, .owner = "runner/heartbeat.innerRunnerHeartbeat", .body = "{}" },
    .{ .method = .GET, .path = MEMORY, .token = RUNNER_TOKEN, .owner = "runner/memory.innerRunnerMemoryHydrate" },
    // `MemoryPushRequest` carries no optional field, and `lease_id` is
    // UUIDv7-checked before the acquire; an empty delta list clears both.
    .{ .method = .POST, .path = MEMORY, .token = RUNNER_TOKEN, .owner = "runner/memory.innerRunnerMemoryCapture", .body = "{\"lease_id\":\"" ++ ABSENT_LEASE_ID ++ "\",\"fencing_token\":1,\"memory\":[]}" },
    // Parsed without `ignore_unknown_fields`, so the body carries exactly
    // `lease_id` + `integration`; `scope` has a default.
    .{ .method = .POST, .path = ME ++ "/credentials/mint", .token = RUNNER_TOKEN, .owner = "runner/credentials_mint.loadMintInputs", .body = "{\"lease_id\":\"" ++ ABSENT_LEASE_ID ++ "\",\"integration\":\"static\"}" },
};

test "integration: test_runner_plane_pool_exhaustion_answers_unavailable — every runner-authed acquire arm answers 503" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // The mint handler refuses with a broker-not-configured envelope before its
    // acquire when `ctx.broker` is null, so the arm needs one wired. The
    // production registry over `nullDeps()` suffices — the starved acquire
    // returns before anything is minted.
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;

    var held: starve.Held = .empty;
    // Released explicitly by the defer below; registered before the drain so a
    // part-way failure cannot strand the connections taken so far.
    defer starve.releaseAll(h, &held);
    try starve.drainPool(h, &held);

    try starve.probeAll(h, &PROBES);
}
