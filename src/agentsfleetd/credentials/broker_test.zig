//! Tests for the credential broker — extracted from broker.zig (FLL: production
//! files stay ≤350 lines; `_test.zig` is exempt). Covers dispatch, caching,
//! expiry, metrics, the unknown path, and concurrent parallel mints over the
//! cache.zig store.

const std = @import("std");
const common = @import("common");
const integration = @import("integration.zig");
const testing = @import("testing.zig");
const CredentialBroker = @import("broker.zig");
const Spec = integration.Spec;
const EXPIRY_SKEW_MS = CredentialBroker.EXPIRY_SKEW_MS;

fn brokerWith(alloc: std.mem.Allocator, registry: []const Spec) !CredentialBroker {
    return CredentialBroker.init(alloc, registry, integration.nullDeps());
}

// A fake integration standing in for github: atomically counts mints and (when
// `delay_ns` is set) sleeps inside the mint so concurrent minters genuinely overlap.
var fake_calls = std.atomic.Value(usize).init(0);
var fake_delay_ns: u64 = 0;
const FAKE_EXPIRY_MS: i64 = 1_000_000;

fn fakeMint(ctx: integration.MintCtx) anyerror!integration.Outcome {
    _ = fake_calls.fetchAdd(1, .monotonic);
    if (fake_delay_ns != 0) common.sleepNanos(fake_delay_ns);
    return .{ .ok = .{ .token = try ctx.alloc.dupe(u8, "minted_tok"), .expires_at_ms = FAKE_EXPIRY_MS } };
}

const FAKE_REGISTRY: []const Spec = &.{.{ .id = .github, .mint = .{ .custom = fakeMint } }};

test "mint: dispatches by id to the matching integration (Dimension 1.1)" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    var b = try brokerWith(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();

    const r = try b.mint(alloc, "ws1", "github", h.value, 0);
    try std.testing.expect(r == .ok);
    defer alloc.free(r.ok.token);
    try std.testing.expectEqualStrings("minted_tok", r.ok.token);
    try std.testing.expectEqual(@as(usize, 1), fake_calls.load(.monotonic));
}

test "mint: an injected descriptor drives dispatch, independent of production (Dimension 1.2 — data-driven)" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    var b = try brokerWith(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();
    const r = try b.mint(alloc, "ws1", "github", h.value, 0);
    try std.testing.expect(r == .ok);
    defer alloc.free(r.ok.token);
    try std.testing.expectEqualStrings("minted_tok", r.ok.token);
}

test "mint: reuses a cached token within validity, re-mints past the skew (Dimension 1.3)" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    var b = try brokerWith(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();

    const r1 = try b.mint(alloc, "ws1", "github", h.value, 0); // miss → mint
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "github", h.value, FAKE_EXPIRY_MS - EXPIRY_SKEW_MS - 1); // hit
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 1), fake_calls.load(.monotonic));

    const r3 = try b.mint(alloc, "ws1", "github", h.value, FAKE_EXPIRY_MS - EXPIRY_SKEW_MS + 1); // re-mint
    alloc.free(r3.ok.token);
    try std.testing.expectEqual(@as(usize, 2), fake_calls.load(.monotonic));
}

// Regression (bounds): the production `static` integration mints with
// expires_at_ms = maxInt(i64) (never expires). ttlSeconds must clamp to a
// BOUNDED MAX_TTL_S, never maxInt(u32) — cache.zig stores the entry expiry as
// `@as(u32, now_epoch_seconds) + ttl` (segment.zig), so a maxInt(u32) ttl
// overflows that u32 add and panics at put time. cache.zig reads the REAL wall
// clock for `now`, so this reproduces regardless of the broker `now_ms` arg.
const NEVER_EXPIRES_MS: i64 = std.math.maxInt(i64);

fn fakeMintNeverExpires(ctx: integration.MintCtx) anyerror!integration.Outcome {
    return .{ .ok = .{ .token = try ctx.alloc.dupe(u8, "static_tok"), .expires_at_ms = NEVER_EXPIRES_MS } };
}

const NEVER_EXPIRES_REGISTRY: []const Spec = &.{.{ .id = .github, .mint = .{ .custom = fakeMintNeverExpires } }};

test "mint: a never-expires token caches without overflowing the cache TTL (bounds regression)" {
    const alloc = std.testing.allocator;
    var b = try brokerWith(alloc, NEVER_EXPIRES_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();

    // Pre-fix this panics with `integer overflow` inside cache.zig's put (real
    // now_epoch_seconds + maxInt(u32)). Post-fix the ttl is clamped to MAX_TTL_S.
    const r = try b.mint(alloc, "ws-static", "github", h.value, 0);
    try std.testing.expect(r == .ok);
    alloc.free(r.ok.token);
    try std.testing.expectEqual(NEVER_EXPIRES_MS, r.ok.expires_at_ms);
}

test "mint: unknown / unregistered id returns unknown_integration, no upstream call (Dimension 1.4)" {
    const alloc = std.testing.allocator;
    var b = try brokerWith(alloc, integration.REGISTRY);
    defer b.deinit();

    // `datadog` is an api_key connector — used directly, never broker-minted — so
    // it is not a broker integration id and resolves to unknown.
    var h1 = try testing.parse(alloc, "{\"integration\":\"datadog\"}");
    defer h1.deinit();
    try std.testing.expect((try b.mint(alloc, "ws1", "datadog", h1.value, 0)) == .unknown_integration);

    var h2 = try testing.parse(alloc, "{\"token\":\"x\"}");
    defer h2.deinit();
    try std.testing.expect((try b.mint(alloc, "ws1", "github", h2.value, 0)) == .unknown_integration);
}

test "mint: an oauth2_refresh token caches within validity, re-mints past the skew (Dimension 3.1)" {
    const alloc = std.testing.allocator;
    const EXPIRES_IN_S: i64 = 3600;
    const EXPIRES_IN_TEXT = std.fmt.comptimePrint("{d}", .{EXPIRES_IN_S});
    const MS_PER_S: i64 = 1000;
    // The production zoho entry mints via the injected token endpoint (FakeGitHub).
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_zoho\",\"expires_in\":" ++ EXPIRES_IN_TEXT ++ "}" };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_abc\"}");
    defer h.deinit();

    const EXPIRY_MS: i64 = EXPIRES_IN_S * MS_PER_S;

    const r1 = try b.mint(alloc, "ws1", "zoho", h.value, 0); // miss → one exchange
    try std.testing.expect(r1 == .ok);
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "zoho", h.value, EXPIRY_MS - EXPIRY_SKEW_MS - 1); // still valid → cache hit
    try std.testing.expect(r2 == .ok);
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 1), vendor.calls); // one exchange served two mints

    const r3 = try b.mint(alloc, "ws1", "zoho", h.value, EXPIRY_MS - EXPIRY_SKEW_MS + 1); // past skew → re-mint
    try std.testing.expect(r3 == .ok);
    alloc.free(r3.ok.token);
    try std.testing.expectEqual(@as(usize, 2), vendor.calls);
}

test "mint: emits a metrics event per call with the cache-hit flag (#11)" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, FAKE_REGISTRY, .{
        .platform = .{},
        .http = integration.nullDeps().http,
        .sign = testing.fakeSign,
        .metrics = rec.sink(),
    });
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();

    const r1 = try b.mint(alloc, "ws1", "github", h.value, 0); // miss
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "github", h.value, 0); // hit
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 2), rec.count);
    try std.testing.expect(rec.last_hit);
    try std.testing.expectEqualStrings("ok", rec.last_outcome);
}

test "mint: different workspaces mint concurrently in parallel over the sharded cache" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    fake_delay_ns = 2 * std.time.ns_per_ms; // make the mints genuinely overlap
    defer fake_delay_ns = 0;
    var b = try brokerWith(alloc, FAKE_REGISTRY);
    defer b.deinit();

    const N = 16;
    const Runner = struct {
        fn go(broker: *CredentialBroker, a: std.mem.Allocator, ws: []const u8) void {
            var h = testing.parse(a, "{\"integration\":\"github\"}") catch return;
            defer h.deinit();
            const r = broker.mint(a, ws, "github", h.value, 0) catch return;
            if (r == .ok) a.free(r.ok.token);
        }
    };
    var ws_bufs: [N][8]u8 = undefined;
    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        const ws = std.fmt.bufPrint(&ws_bufs[i], "ws{d}", .{i}) catch unreachable;
        t.* = try std.Thread.spawn(.{}, Runner.go, .{ &b, alloc, ws });
    }
    for (threads) |t| t.join();

    // Distinct keys → distinct segments → all minted in parallel, none lost.
    try std.testing.expectEqual(@as(usize, N), fake_calls.load(.monotonic));
}

// ── Identity fingerprint + rotated-refresh threading ─────────────────────────

// Shared vendor-response bodies for the oauth2_refresh fingerprint/rotation
// tests: a plain exchange and one that rotates the refresh token.
const FP_EXPIRES_IN_S: i64 = 3600;
const FP_EXPIRES_IN_TEXT = std.fmt.comptimePrint("{d}", .{FP_EXPIRES_IN_S});
const PLAIN_EXCHANGE_RESP = "{\"access_token\":\"at_1\",\"expires_in\":" ++ FP_EXPIRES_IN_TEXT ++ "}";
const ROTATING_EXCHANGE_RESP = "{\"access_token\":\"at_1\",\"expires_in\":" ++ FP_EXPIRES_IN_TEXT ++ ",\"refresh_token\":\"rt_new\"}";

// Fixture twin of the github handle's identity field (the production const is
// module-private); the fake below derives its token from it so a wrong-cache-hit
// is caught by token VALUE, not just exchange count.
const F_INSTALLATION_ID = "installation_id";
const HANDLE_INSTALL_X = "{\"integration\":\"github\",\"" ++ F_INSTALLATION_ID ++ "\":\"inst_x\"}";
const HANDLE_INSTALL_Y = "{\"integration\":\"github\",\"" ++ F_INSTALLATION_ID ++ "\":\"inst_y\"}";

/// Mints `tok_<installation_id>` so each handle identity has a distinct token.
fn fakeMintFromInstallation(ctx: integration.MintCtx) anyerror!integration.Outcome {
    _ = fake_calls.fetchAdd(1, .monotonic);
    const inst = switch (ctx.handle.object.get(F_INSTALLATION_ID).?) {
        .string => |s| s,
        else => return .{ .mint_failed = .permanent },
    };
    const tok = try std.fmt.allocPrint(ctx.alloc, "tok_{s}", .{inst});
    return .{ .ok = .{ .token = tok, .expires_at_ms = FAKE_EXPIRY_MS } };
}

const INSTALL_REGISTRY: []const Spec = &.{.{ .id = .github, .mint = .{ .custom = fakeMintFromInstallation } }};

test "mint: test_reconnect_identity_change_remints — a changed identity field misses the cache (github + static shapes)" {
    const alloc = std.testing.allocator;
    // github shape: same (workspace, integration), different installation —
    // the second mint must return the NEW installation's token, never inst_x's
    // cached one (the pre-fix bug: a reconnect kept serving the old token).
    {
        fake_calls.store(0, .monotonic);
        var b = try brokerWith(alloc, INSTALL_REGISTRY);
        defer b.deinit();
        var hx = try testing.parse(alloc, HANDLE_INSTALL_X);
        defer hx.deinit();
        var hy = try testing.parse(alloc, HANDLE_INSTALL_Y);
        defer hy.deinit();

        const r1 = try b.mint(alloc, "ws1", "github", hx.value, 0);
        defer alloc.free(r1.ok.token);
        try std.testing.expectEqualStrings("tok_inst_x", r1.ok.token);
        const r2 = try b.mint(alloc, "ws1", "github", hy.value, 0);
        defer alloc.free(r2.ok.token);
        try std.testing.expectEqualStrings("tok_inst_y", r2.ok.token);
        try std.testing.expectEqual(@as(usize, 2), fake_calls.load(.monotonic));
    }
    // static shape: a rotated Personal Access Token is a non-excluded `token`
    // field change → immediate re-mint (no 24h staleness window), through the
    // PRODUCTION registry's static integration.
    {
        var b = try brokerWith(alloc, integration.REGISTRY);
        defer b.deinit();
        var h0 = try testing.parse(alloc, "{\"integration\":\"static\",\"token\":\"pat_t0\"}");
        defer h0.deinit();
        var h1 = try testing.parse(alloc, "{\"integration\":\"static\",\"token\":\"pat_t1\"}");
        defer h1.deinit();

        const r1 = try b.mint(alloc, "ws1", "static", h0.value, 0);
        defer alloc.free(r1.ok.token);
        try std.testing.expectEqualStrings("pat_t0", r1.ok.token);
        // Pre-fix this returned the cached pat_t0 (static never expires).
        const r2 = try b.mint(alloc, "ws1", "static", h1.value, 0);
        defer alloc.free(r2.ok.token);
        try std.testing.expectEqualStrings("pat_t1", r2.ok.token);
    }
}

test "mint: test_refresh_only_change_hits_cache — a rotated refresh token is NOT an identity change" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = PLAIN_EXCHANGE_RESP };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    var h0 = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_0\"}");
    defer h0.deinit();
    var h1 = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_1\"}");
    defer h1.deinit();

    const r1 = try b.mint(alloc, "ws1", "zoho", h0.value, 0); // miss → one exchange
    try std.testing.expect(r1 == .ok);
    alloc.free(r1.ok.token);
    // Only the excluded rotating field differs → same fingerprint → cache hit.
    const r2 = try b.mint(alloc, "ws1", "zoho", h1.value, 0);
    try std.testing.expect(r2 == .ok);
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 1), vendor.calls);
}

test "mint: test_fingerprint_canonical_order — JSON key order does not change the cache key" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = PLAIN_EXCHANGE_RESP };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    // Same fields, different insertion order (std.json preserves parse order).
    var ha = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt\",\"label\":\"L\",\"accounts_base\":\"https://a.test\"}");
    defer ha.deinit();
    var hb = try testing.parse(alloc, "{\"accounts_base\":\"https://a.test\",\"label\":\"L\",\"integration\":\"zoho\",\"refresh_token\":\"rt\"}");
    defer hb.deinit();

    const r1 = try b.mint(alloc, "ws1", "zoho", ha.value, 0);
    try std.testing.expect(r1 == .ok);
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "zoho", hb.value, 0);
    try std.testing.expect(r2 == .ok);
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 1), vendor.calls); // one exchange served both
}

test "mint: test_broker_threads_rotated_on_miss_only — rotated token rides the cold path, null on a hit" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = ROTATING_EXCHANGE_RESP };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_old\"}");
    defer h.deinit();

    const r1 = try b.mint(alloc, "ws1", "zoho", h.value, 0); // miss → exchange rotated
    try std.testing.expect(r1 == .ok);
    defer alloc.free(r1.ok.token);
    try std.testing.expect(r1.ok.rotated_refresh_token != null);
    defer alloc.free(r1.ok.rotated_refresh_token.?);
    try std.testing.expectEqualStrings("rt_new", r1.ok.rotated_refresh_token.?);

    const r2 = try b.mint(alloc, "ws1", "zoho", h.value, 0); // hit → no exchange
    try std.testing.expect(r2 == .ok);
    defer alloc.free(r2.ok.token);
    try std.testing.expect(r2.ok.rotated_refresh_token == null);
    try std.testing.expectEqual(@as(usize, 1), vendor.calls);
}

test "mint: should fail closed when the key (workspace + id + fingerprint) overflows the key buffer" {
    const alloc = std.testing.allocator;
    fake_calls.store(0, .monotonic);
    var b = try brokerWith(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();

    // Two boundary shapes: grossly over (600), and the honest-guard edge where
    // workspace + id + separators fit but the 16-hex fingerprint would not
    // (500 + "github" + 2 + 16 = 524 > 512) — the pre-review guard passed this
    // one to a bufPrint failure; the guard now reserves the full key up front.
    inline for (.{ "w" ** 600, "w" ** 500 }) |giant_ws| {
        const r = try b.mint(alloc, giant_ws, "github", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.permanent, r.mint_failed);
    }
    // Fails BEFORE any upstream call — a broken key never mints.
    try std.testing.expectEqual(@as(usize, 0), fake_calls.load(.monotonic));
}

test "mint: test_reconnect_refresh_provider_remints — a fresh connect stamp misses the cache when every other identity field is a constant" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = PLAIN_EXCHANGE_RESP };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    // Linear-shaped worst case: every non-rotating field except connected_at_ms
    // is byte-identical between grant A and grant B (a real Linear handle's
    // label is a constant). Only the connect callback's connected_at_ms stamp
    // distinguishes the grants — pre-stamp the fingerprint was constant, the
    // second mint HIT the cache, and grant A's token kept serving after the
    // operator reconnected as grant B.
    var ha = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_a\",\"label\":\"L\",\"connected_at_ms\":111}");
    defer ha.deinit();
    var hb = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_b\",\"label\":\"L\",\"connected_at_ms\":222}");
    defer hb.deinit();

    const r1 = try b.mint(alloc, "ws1", "zoho", ha.value, 0);
    try std.testing.expect(r1 == .ok);
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "zoho", hb.value, 0);
    try std.testing.expect(r2 == .ok);
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 2), vendor.calls); // reconnect re-minted
}

test "mint: test_broker_rotated_token_ownership — one free path per copy, fail-closed on partial dupe (OOM)" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = ROTATING_EXCHANGE_RESP };
    defer vendor.deinit();
    var rec = testing.RecordingMetrics{};
    // Broker internals run on the leak-detecting testing allocator: the strategy
    // copies the broker frees and the cache copy are all audited at deinit.
    var b = try CredentialBroker.init(alloc, integration.REGISTRY, testing.brokerDeps(&vendor, &rec));
    defer b.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"zoho\",\"refresh_token\":\"rt_old\"}");
    defer h.deinit();

    // Success path: the caller frees its own two copies; zero leaks overall.
    {
        const r = try b.mint(alloc, "ws-own", "zoho", h.value, 0);
        try std.testing.expect(r == .ok);
        alloc.free(r.ok.token);
        alloc.free(r.ok.rotated_refresh_token.?);
    }
    // Partial-dupe OOM: the caller's token dupe succeeds (index 0), the rotated
    // dupe fails (index 1) → the token copy is freed on the fail-closed path,
    // so nothing leaks and no double-free fires (RULE OWN).
    {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 1 });
        const r = try b.mint(failing.allocator(), "ws-own-2", "zoho", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.transient, r.mint_failed);
    }
}

test "mint: a caller allocation failure fails closed as mint_failed{transient}, no leak (OOM injection)" {
    fake_calls.store(0, .monotonic);
    // Broker internals (cache + runMint copy) use the non-failing testing
    // allocator; only the mint's CALLER allocator fails — isolating the
    // caller-facing token dup, the one alloc that can OOM a successful mint.
    var b = try brokerWith(std.testing.allocator, FAKE_REGISTRY);
    defer b.deinit();
    var h = try testing.parse(std.testing.allocator, "{\"integration\":\"github\"}");
    defer h.deinit();

    // Cold path: runMint succeeds, the caller dup OOMs → mint_failed{transient}.
    // The failed mint leaves NO warm entry (cache-last): a hit reports no
    // rotated token, so caching a mint the caller never received would strand
    // rotation state.
    {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
        const r = try b.mint(failing.allocator(), "ws-oom", "github", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.transient, r.mint_failed);
    }
    // Warm the cache with a successful mint…
    {
        const r = try b.mint(std.testing.allocator, "ws-oom", "github", h.value, 0);
        try std.testing.expect(r == .ok);
        std.testing.allocator.free(r.ok.token);
    }
    // …then a HIT whose caller dup OOMs takes the SAME fail-closed branch —
    // never a panic, never a stale token.
    {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
        const r = try b.mint(failing.allocator(), "ws-oom", "github", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.transient, r.mint_failed);
    }
    // Two upstream mints: the OOM'd cold attempt (uncached, by design) and the
    // successful warm-up; the final OOM'd call was a cache hit. The testing
    // allocator's deinit asserts every internal allocation was freed.
    try std.testing.expectEqual(@as(usize, 2), fake_calls.load(.monotonic));
}
