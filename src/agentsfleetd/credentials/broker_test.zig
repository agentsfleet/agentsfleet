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
    {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
        const r = try b.mint(failing.allocator(), "ws-oom", "github", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.transient, r.mint_failed);
    }
    // Warm path: the cold mint above cached ws-oom; a hit whose caller dup OOMs
    // takes the SAME fail-closed branch — never a panic, never a stale token.
    {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
        const r = try b.mint(failing.allocator(), "ws-oom", "github", h.value, 0);
        try std.testing.expect(r == .mint_failed);
        try std.testing.expectEqual(integration.Retry.transient, r.mint_failed);
    }
    // Exactly one upstream mint (the cold one); the warm path hit cache. The
    // testing allocator's deinit asserts every internal allocation was freed.
    try std.testing.expectEqual(@as(usize, 1), fake_calls.load(.monotonic));
}
