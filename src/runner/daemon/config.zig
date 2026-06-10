//! Runner daemon startup configuration — read once from the environment at
//! launch, before any control-plane contact.
//!
//! Env var naming follows the ZOMBIE_ namespace convention used by zombied; the
//! RUNNER_ prefix scopes variables that are runner-only and have no counterpart
//! in zombied's config. All vars are required unless a default is documented.
//!
//! File-as-struct: the file IS the `Config` value. All slices are owned by the
//! allocator passed to `load()`; call `deinit()` when done. Datastore-free
//! (string slices only) so it links cleanly into the runner build graph, which
//! deliberately omits pg/redis.

const Config = @This();

/// Base URL of the zombied control plane, e.g. `http://127.0.0.1:8080`.
control_plane_url: []const u8,
/// Pre-minted runner token (`zrn_…`) the platform operator installed on this
/// host via `ZOMBIE_RUNNER_TOKEN`. Authenticates every control-plane call; the
/// host never self-registers (Option B). Prefix-validated at load; never logged.
runner_token: []const u8,
/// Stable machine identifier, logged for operator correlation. The fleet row's
/// host_id is set server-side when the operator pre-mints the token.
host_id: []const u8,
/// Self-reported isolation tier the daemon enforces locally (the dev_none gate
/// + sandbox setup). Defaults to `dev_none`.
sandbox_tier: []const u8,
/// Base directory under which per-lease workspace subdirs are created.
workspace_base: []const u8,
/// Egress policy for sandboxed leases (`RUNNER_NETWORK_POLICY`), resolved once
/// at load. sandbox_args owns the `--share-net` decision and reads it per-lease
/// off `cfg`; Zig 0.16 routes the env read through `Environ.Map` at startup,
/// so the daemon hot path never touches the environment.
network_policy: network.PolicyMode,
/// Number of concurrent worker threads the daemon runs (env
/// `RUNNER_WORKER_COUNT`). Each worker independently leases → executes → reports;
/// the per-zombie `affinity.claim` keeps two off the same zombie. Default 1 is
/// today's single-agent-per-host behaviour; clamped to `[1, MAX_WORKER_COUNT]`
/// so a fat-fingered value can't fork unbounded children. Capacity-aware sizing
/// is out of scope — the operator sizes N to the host.
worker_count: u32,
/// Operator-fed registry baseline (env `RUNNER_REGISTRY_ALLOWLIST`,
/// comma-separated), merged into each lease's egress allowlist. Empty when unset
/// — the caller substitutes the named default (`network/AllowList.DEFAULT_REGISTRY`).
/// Fed from outside, never a compile-time list.
registry_allowlist: []const []const u8,

alloc: Allocator,

pub const ConfigError = error{ MissingEnvVar, InvalidRunnerToken, OutOfMemory };

/// Read configuration from the process environment. Returns
/// `ConfigError.MissingEnvVar` for required vars that are absent, and
/// `ConfigError.InvalidRunnerToken` when the token lacks the `zrn_` prefix.
pub fn load(env_map: *const std.process.Environ.Map, alloc: Allocator) ConfigError!Config {
    const url = getRequired(env_map, alloc, ENV_ZOMBIE_API_URL) catch
        return ConfigError.MissingEnvVar;
    errdefer alloc.free(url);

    const token = getRequired(env_map, alloc, ENV_ZOMBIE_RUNNER_TOKEN) catch
        return ConfigError.MissingEnvVar;
    errdefer alloc.free(token);
    try assertRunnerTokenPrefix(token);

    const host_id = getRequired(env_map, alloc, ENV_RUNNER_HOST_ID) catch
        return ConfigError.MissingEnvVar;
    errdefer alloc.free(host_id);

    const tier = (getOwned(env_map, alloc, ENV_RUNNER_SANDBOX_TIER) catch null) orelse
        (alloc.dupe(u8, DEFAULT_SANDBOX_TIER) catch return ConfigError.OutOfMemory);
    errdefer alloc.free(tier);

    const workspace_base = (getOwned(env_map, alloc, ENV_RUNNER_WORKSPACE_BASE) catch null) orelse
        (alloc.dupe(u8, DEFAULT_WORKSPACE_BASE) catch return ConfigError.OutOfMemory);
    errdefer alloc.free(workspace_base);

    const worker_count_raw = getOwned(env_map, alloc, ENV_RUNNER_WORKER_COUNT) catch null;
    defer if (worker_count_raw) |raw| alloc.free(raw);
    const worker_count = switch (parseWorkerCount(worker_count_raw)) {
        .value => |v| v,
        .invalid => blk: {
            log.warn("runner_worker_count_invalid", .{ .raw = worker_count_raw.?, .fallback = DEFAULT_WORKER_COUNT });
            break :blk DEFAULT_WORKER_COUNT;
        },
    };

    const registry_raw = getOwned(env_map, alloc, ENV_RUNNER_REGISTRY_ALLOWLIST) catch null;
    defer if (registry_raw) |raw| alloc.free(raw);
    const registry_allowlist = parseRegistryAllowlist(alloc, registry_raw) catch return ConfigError.OutOfMemory;

    return Config{
        .control_plane_url = url,
        .runner_token = token,
        .host_id = host_id,
        .sandbox_tier = tier,
        .workspace_base = workspace_base,
        .network_policy = network.policyFromMap(env_map),
        .worker_count = worker_count,
        .registry_allowlist = registry_allowlist,
        .alloc = alloc,
    };
}

/// Result of reading `RUNNER_WORKER_COUNT`: a usable clamped count, or `.invalid`
/// when a present value does not parse (the caller falls back + warns). Unset is
/// `.value = DEFAULT_WORKER_COUNT`, never `.invalid`.
pub const WorkerCountParse = union(enum) { value: u32, invalid };

/// Pure parse+clamp for `RUNNER_WORKER_COUNT` (RULE UFS: bounds single-sourced).
/// null/unset → default; non-numeric/empty → `.invalid`; `0`/over-MAX → clamped
/// into `[MIN_WORKER_COUNT, MAX_WORKER_COUNT]`. No logging, so it is unit-testable.
fn parseWorkerCount(raw: ?[]const u8) WorkerCountParse {
    const s = raw orelse return .{ .value = DEFAULT_WORKER_COUNT };
    const trimmed = std.mem.trim(u8, s, &std.ascii.whitespace);
    const n = std.fmt.parseInt(u32, trimmed, 10) catch return .invalid;
    return .{ .value = std.math.clamp(n, MIN_WORKER_COUNT, MAX_WORKER_COUNT) };
}

/// Parse `RUNNER_REGISTRY_ALLOWLIST` (comma-separated) into owned hostnames:
/// whitespace-trimmed, empty tokens skipped. Null/unset → empty slice (the
/// caller substitutes the named default). The operator feeds this from outside;
/// it is never a compile-time list. Caller owns the result (`freeStrList`).
fn parseRegistryAllowlist(alloc: Allocator, raw: ?[]const u8) Allocator.Error![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |x| alloc.free(x);
        list.deinit(alloc);
    }
    if (raw) |s| {
        var it = std.mem.splitScalar(u8, s, ',');
        while (it.next()) |tok| {
            const trimmed = std.mem.trim(u8, tok, &std.ascii.whitespace);
            if (trimmed.len == 0) continue;
            const owned = try alloc.dupe(u8, trimmed);
            errdefer alloc.free(owned);
            try list.append(alloc, owned);
        }
    }
    return list.toOwnedSlice(alloc);
}

fn freeStrList(alloc: Allocator, list: []const []const u8) void {
    for (list) |x| alloc.free(x);
    alloc.free(list);
}

pub fn deinit(self: Config) void {
    self.alloc.free(self.control_plane_url);
    self.alloc.free(self.runner_token);
    self.alloc.free(self.host_id);
    self.alloc.free(self.sandbox_tier);
    self.alloc.free(self.workspace_base);
    freeStrList(self.alloc, self.registry_allowlist);
}

/// Fail loud when `ZOMBIE_RUNNER_TOKEN` is not a `zrn_` runner token — a stale
/// `zmb_t_` from the pre-Option-B bootstrap would otherwise loop on 401s with
/// no clear cause. Pure so the prefix contract is unit-testable without env.
fn assertRunnerTokenPrefix(token: []const u8) ConfigError!void {
    if (!std.mem.startsWith(u8, token, contract.protocol.RUNNER_TOKEN_PREFIX))
        return ConfigError.InvalidRunnerToken;
}

fn getRequired(env_map: *const std.process.Environ.Map, alloc: Allocator, name: []const u8) ![]u8 {
    return (try getOwned(env_map, alloc, name)) orelse error.MissingEnvVar;
}

/// Owned copy of env var `name`, or null when unset. Only OOM propagates — a
/// missing var is null (never an error), so callers choose required-vs-default.
/// Zig 0.16 removed `std.process.getEnvVarOwned`; the environment block is
/// handed to `main` via `Init` and threaded here as a pre-built `Environ.Map`.
fn getOwned(env_map: *const std.process.Environ.Map, alloc: Allocator, name: []const u8) Allocator.Error!?[]u8 {
    const value = env_map.get(name) orelse return null;
    return try alloc.dupe(u8, value);
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const contract = @import("contract");
const network = @import("../engine/network.zig");
const logging = @import("log");

const log = logging.scoped(.zombie_runner);

/// Environment variable names — single-sourced (RULE UFS).
pub const ENV_ZOMBIE_API_URL = "ZOMBIE_API_URL";
pub const ENV_ZOMBIE_RUNNER_TOKEN = "ZOMBIE_RUNNER_TOKEN";
pub const ENV_RUNNER_HOST_ID = "RUNNER_HOST_ID";
pub const ENV_RUNNER_SANDBOX_TIER = "RUNNER_SANDBOX_TIER";
pub const ENV_RUNNER_WORKSPACE_BASE = "RUNNER_WORKSPACE_BASE";
pub const ENV_RUNNER_WORKER_COUNT = "RUNNER_WORKER_COUNT";
pub const ENV_RUNNER_REGISTRY_ALLOWLIST = "RUNNER_REGISTRY_ALLOWLIST";

// Derived from the SandboxTier enum (RULE UFS: single source). dev_none is the
// only tier that runs without isolation — dev default; prod must override.
const DEFAULT_SANDBOX_TIER = @tagName(contract.protocol.SandboxTier.dev_none);
const DEFAULT_WORKSPACE_BASE = "/tmp/zombie-runner";

/// Worker-pool sizing bounds (RULE UFS: the clamp is single-sourced). Default 1
/// = today's one-agent-per-host daemon; MAX caps a misconfigured value so the
/// pool can never fork unbounded children on one host.
pub const DEFAULT_WORKER_COUNT: u32 = 1;
pub const MIN_WORKER_COUNT: u32 = 1;
pub const MAX_WORKER_COUNT: u32 = 64;

test "assertRunnerTokenPrefix accepts zrn_ tokens, rejects everything else" {
    try assertRunnerTokenPrefix("zrn_" ++ "a" ** 64);
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("zmb_t_deadbeef"));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix(""));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("zrn"));
}

test "worker count parses default and clamps" {
    try std.testing.expectEqual(DEFAULT_WORKER_COUNT, parseWorkerCount(null).value); // unset → default
    try std.testing.expectEqual(@as(u32, 8), parseWorkerCount("8").value);
    try std.testing.expectEqual(MIN_WORKER_COUNT, parseWorkerCount("0").value); // below floor → clamp up
    try std.testing.expectEqual(MAX_WORKER_COUNT, parseWorkerCount("99999").value); // above ceiling → clamp down
    try std.testing.expectEqual(@as(u32, 4), parseWorkerCount("  4 \n").value); // surrounding whitespace tolerated
}

test "worker count invalid falls back to default" {
    try std.testing.expect(parseWorkerCount("abc") == .invalid); // non-numeric → caller defaults + warns
    try std.testing.expect(parseWorkerCount("") == .invalid); // empty → invalid, never a silent 0
    try std.testing.expect(parseWorkerCount("-3") == .invalid); // signed rejected by u32 parse
}

test "parseRegistryAllowlist splits, trims, and skips empty tokens" {
    const a = std.testing.allocator;
    const r = try parseRegistryAllowlist(a, "registry.npmjs.org, pypi.org ,, crates.io");
    defer freeStrList(a, r);
    try std.testing.expectEqual(@as(usize, 3), r.len);
    try std.testing.expectEqualStrings("registry.npmjs.org", r[0]);
    try std.testing.expectEqualStrings("pypi.org", r[1]);
    try std.testing.expectEqualStrings("crates.io", r[2]);
}

test "parseRegistryAllowlist on null or whitespace-only yields an empty slice" {
    const a = std.testing.allocator;
    const r1 = try parseRegistryAllowlist(a, null); // unset → caller substitutes the default
    defer freeStrList(a, r1);
    try std.testing.expectEqual(@as(usize, 0), r1.len);
    const r2 = try parseRegistryAllowlist(a, "  ,  ");
    defer freeStrList(a, r2);
    try std.testing.expectEqual(@as(usize, 0), r2.len);
}
