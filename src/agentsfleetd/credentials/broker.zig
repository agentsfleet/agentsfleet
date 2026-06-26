//! On-demand credential broker. Resolves a vault handle to a short-lived token
//! by dispatching to the config-driven driver registry (`driver.zig`), caching
//! the result until near expiry. Adding a connector is a registry descriptor,
//! never a branch here (RULE CFG).
//!
//! RESOURCE BUDGET: the cache holds one entry per (workspace, integration),
//! overwritten on re-mint — bounded by tenants × integrations, not per-request.
//! Tokens are small and duped once; lookups copy the value out, never alias.

const CredentialBroker = @This();

/// Re-mint this many ms BEFORE the upstream expiry so a token handed to a tool
/// call has slack to complete (RULE UFS).
const EXPIRY_SKEW_MS: i64 = 60_000;

/// Cache-key separator joining (workspace, integration). ASCII unit separator —
/// never present in either field, so key boundaries cannot collide.
const KEY_SEP: u8 = 0x1f;

alloc: std.mem.Allocator,
registry: []const Driver,
cache: std.StringHashMapUnmanaged(driver.Minted) = .empty,

/// `registry` is injected (production passes `driver.DRIVER_REGISTRY`) so a test
/// can supply a fake-kind registry and prove dispatch is data-driven.
pub fn init(alloc: std.mem.Allocator, registry: []const Driver) CredentialBroker {
    return .{ .alloc = alloc, .registry = registry };
}

pub fn deinit(self: *CredentialBroker) void {
    var it = self.cache.iterator();
    while (it.next()) |e| {
        self.alloc.free(e.key_ptr.*);
        self.alloc.free(e.value_ptr.token);
    }
    self.cache.deinit(self.alloc);
    self.* = undefined;
}

/// Resolve `integration` for `workspace` to a short-lived token, minting via the
/// driver registry on a cache miss. `now_ms` is injected (production passes
/// `clock.nowMillis()`) for deterministic expiry. The returned `ok.token` is
/// duped with `alloc` (caller-owned) — never an alias into the cache.
pub fn mint(
    self: *CredentialBroker,
    alloc: std.mem.Allocator,
    workspace: []const u8,
    integration: []const u8,
    handle: std.json.Value,
    now_ms: i64,
) !driver.MintResult {
    var key_buf: [512]u8 = undefined;
    const key = writeKey(&key_buf, workspace, integration) orelse return .mint_failed;

    if (self.cache.get(key)) |hit| {
        if (now_ms < hit.expires_at_ms - EXPIRY_SKEW_MS) {
            return .{ .ok = .{ .token = try alloc.dupe(u8, hit.token), .expires_at_ms = hit.expires_at_ms } };
        }
    }

    const kind = parseKind(handle) orelse return .unknown_integration;
    const drv = driver.resolve(self.registry, kind) orelse return .unknown_integration;

    const outcome = drv.mintFn(self.alloc, handle) catch return .mint_failed;
    switch (outcome) {
        .ok => |minted| {
            self.store(key, minted) catch |err| {
                self.alloc.free(minted.token);
                return err;
            };
            // The cache now owns minted.token; hand the caller an independent copy.
            return .{ .ok = .{ .token = try alloc.dupe(u8, minted.token), .expires_at_ms = minted.expires_at_ms } };
        },
        .reconnect_required => return .reconnect_required,
        .mint_failed => return .mint_failed,
    }
}

/// Insert/overwrite the cache entry. Frees a prior entry's token; dups the key
/// only for a new entry. The minted token is already owned by `self.alloc`.
fn store(self: *CredentialBroker, key: []const u8, minted: driver.Minted) !void {
    const gop = try self.cache.getOrPut(self.alloc, key);
    if (gop.found_existing) {
        self.alloc.free(gop.value_ptr.token);
    } else {
        gop.key_ptr.* = self.alloc.dupe(u8, key) catch |err| {
            self.cache.removeByPtr(gop.key_ptr);
            return err;
        };
    }
    gop.value_ptr.* = minted;
}

fn parseKind(handle: std.json.Value) ?driver.Kind {
    const obj = switch (handle) {
        .object => |o| o,
        else => return null,
    };
    const kv = obj.get(driver.FIELD_KIND) orelse return null;
    const ks = switch (kv) {
        .string => |s| s,
        else => return null,
    };
    return driver.kindFromString(ks);
}

fn writeKey(buf: []u8, workspace: []const u8, integration: []const u8) ?[]const u8 {
    if (workspace.len + integration.len + 1 > buf.len) return null;
    @memcpy(buf[0..workspace.len], workspace);
    buf[workspace.len] = KEY_SEP;
    @memcpy(buf[workspace.len + 1 ..][0..integration.len], integration);
    return buf[0 .. workspace.len + 1 + integration.len];
}

const std = @import("std");
const driver = @import("driver.zig");
const Driver = driver.Driver;

// ── Tests (pure — injected registry + clock, no DB, no upstream) ─────────────

var fake_calls: usize = 0;

/// Fixed expiry the fake driver stamps, so the cache test reasons about the
/// skew boundary against a named value, not a bare literal.
const FAKE_EXPIRY_MS: i64 = 1_000_000;

/// Fake driver standing in for `github_app`: counts invocations and returns a
/// token expiring at a fixed epoch ms, so a test can assert cache reuse.
fn fakeMintFinite(alloc: std.mem.Allocator, handle: std.json.Value) anyerror!driver.DriverOutcome {
    _ = handle;
    fake_calls += 1;
    return .{ .ok = .{ .token = try alloc.dupe(u8, "minted_tok"), .expires_at_ms = FAKE_EXPIRY_MS } };
}

const FAKE_REGISTRY: []const Driver = &.{.{ .kind = .github_app, .mintFn = fakeMintFinite }};

fn parseHandle(alloc: std.mem.Allocator, comptime json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

test "mint: dispatches by kind to the matching driver (Dimension 1.1)" {
    const alloc = std.testing.allocator;
    fake_calls = 0;
    var b = CredentialBroker.init(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try parseHandle(alloc, "{\"kind\":\"github_app\"}");
    defer h.deinit();

    const r = try b.mint(alloc, "ws1", "github", h.value, 0);
    try std.testing.expect(r == .ok);
    defer alloc.free(r.ok.token);
    try std.testing.expectEqualStrings("minted_tok", r.ok.token);
    try std.testing.expectEqual(@as(usize, 1), fake_calls);
}

test "mint: a kind absent from the production registry becomes mintable purely via the injected descriptor (Dimension 1.2 — data-driven)" {
    const alloc = std.testing.allocator;
    // github_app is NOT in driver.DRIVER_REGISTRY in §1 — yet a registry that
    // lists it makes it mintable with zero edit to mint()/resolve().
    try std.testing.expect(driver.resolve(driver.DRIVER_REGISTRY, .github_app) == null);
    fake_calls = 0;
    var b = CredentialBroker.init(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try parseHandle(alloc, "{\"kind\":\"github_app\"}");
    defer h.deinit();
    const r = try b.mint(alloc, "ws1", "github", h.value, 0);
    try std.testing.expect(r == .ok);
    alloc.free(r.ok.token);
}

test "mint: reuses a cached token within validity, re-mints past the skew (Dimension 1.3)" {
    const alloc = std.testing.allocator;
    fake_calls = 0;
    var b = CredentialBroker.init(alloc, FAKE_REGISTRY);
    defer b.deinit();
    var h = try parseHandle(alloc, "{\"kind\":\"github_app\"}");
    defer h.deinit();

    const r1 = try b.mint(alloc, "ws1", "github", h.value, 0); // miss → mint
    alloc.free(r1.ok.token);
    const r2 = try b.mint(alloc, "ws1", "github", h.value, FAKE_EXPIRY_MS - EXPIRY_SKEW_MS - 1); // still valid → hit
    alloc.free(r2.ok.token);
    try std.testing.expectEqual(@as(usize, 1), fake_calls);

    const r3 = try b.mint(alloc, "ws1", "github", h.value, FAKE_EXPIRY_MS - EXPIRY_SKEW_MS + 1); // past skew → re-mint
    alloc.free(r3.ok.token);
    try std.testing.expectEqual(@as(usize, 2), fake_calls);
}

test "mint: unknown / unregistered kind returns unknown_integration, no upstream call (Dimension 1.4)" {
    const alloc = std.testing.allocator;
    var b = CredentialBroker.init(alloc, driver.DRIVER_REGISTRY);
    defer b.deinit();

    // kind not in the enum at all
    var h1 = try parseHandle(alloc, "{\"kind\":\"zoho\"}");
    defer h1.deinit();
    try std.testing.expect((try b.mint(alloc, "ws1", "zoho", h1.value, 0)) == .unknown_integration);

    // valid kind but no driver registered for it in the production registry
    var h2 = try parseHandle(alloc, "{\"kind\":\"github_app\"}");
    defer h2.deinit();
    try std.testing.expect((try b.mint(alloc, "ws1", "github", h2.value, 0)) == .unknown_integration);
}
