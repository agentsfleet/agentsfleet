//! Runner daemon bootstrap configuration — read once from the environment at
//! launch, before any control-plane contact.
//!
//! The environment carries ONLY the bootstrap trio: where the control plane
//! is, who this runner is, and where this host's disk lives. Policy — tier,
//! egress, registry baseline, worker count — is ASSIGNED by the control plane
//! and arrives with the first heartbeat; the policy fields below are
//! fail-closed placeholders that each lease overwrites from the applied
//! assignment (`daemon/AppliedPolicy.zig`). The environment is never read for
//! them, so there is no fallback path for the two sources to diverge through.
//!
//! File-as-struct: the file IS the `Config` value. All slices are owned by the
//! allocator passed to `load()`; call `deinit()` when done. Datastore-free
//! (string slices only) so it links cleanly into the runner build graph, which
//! deliberately omits pg/redis.

const Config = @This();

/// Base URL of the agentsfleetd control plane, e.g. `http://127.0.0.1:8080`.
control_plane_url: []const u8,
/// Pre-minted runner token (`agt_r…`) the platform operator installed on this
/// host via `AGENTSFLEET_RUNNER_TOKEN`. Authenticates every control-plane call
/// AND resolves this runner's identity server-side — the host never
/// self-registers and never declares who it is. Prefix-validated at load;
/// never logged.
runner_token: []const u8,
/// Host-local root for per-lease scratch workspaces and the bundle cache
/// (env `RUNNER_STORAGE_HOME`) — a disk fact the control plane cannot know,
/// which is why it is the one optional environment variable that survives.
storage_home: []const u8,

// === Effective-copy policy fields ===
// Fail-closed placeholders at load; the worker stamps each lease's effective
// copy from the applied assignment before anything downstream reads them.
// Nothing leases while no policy is applied (a null AppliedPolicy refuses),
// and even a bug that read these early would meet the safest posture — the
// un-isolated tier a release build refuses, and the egress mode that refuses
// leases — never a permissive default.
sandbox_tier: contract.protocol.SandboxTier,
network_policy: network.Mode,
worker_count: u32,
/// Never owned by Config: the placeholder is a static empty slice, and each
/// effective copy borrows from an `AppliedPolicy` snapshot the worker frees.
registry_allowlist: []const []const u8,
/// Control-plane call deadlines — code defaults, single-sourced with the
/// client (`call_deadline`). No environment override surface: a deadline is
/// transport plumbing, not operator policy.
cp_deadlines: call_deadline.Deadlines,

alloc: Allocator,

pub const ConfigError = error{ MissingEnvVar, InvalidRunnerToken, OutOfMemory };

/// Read the bootstrap trio from the process environment. Returns
/// `ConfigError.MissingEnvVar` for required vars that are absent, and
/// `ConfigError.InvalidRunnerToken` when the token lacks the `agt_r` prefix.
pub fn load(env_map: *const std.process.Environ.Map, alloc: Allocator) ConfigError!Config {
    const url = getRequired(env_map, alloc, ENV_AGENTSFLEET_API_URL) catch
        return ConfigError.MissingEnvVar;
    errdefer alloc.free(url);

    const token = getRequired(env_map, alloc, ENV_AGENTSFLEET_RUNNER_TOKEN) catch
        return ConfigError.MissingEnvVar;
    errdefer alloc.free(token);
    try assertRunnerTokenPrefix(token);

    const storage_home = (getOwned(env_map, alloc, ENV_RUNNER_STORAGE_HOME) catch null) orelse
        (alloc.dupe(u8, DEFAULT_STORAGE_HOME) catch return ConfigError.OutOfMemory);

    return Config{
        .control_plane_url = url,
        .runner_token = token,
        .storage_home = storage_home,
        .sandbox_tier = .dev_none,
        .network_policy = network.FAIL_CLOSED_DEFAULT,
        .worker_count = contract.protocol.DEFAULT_WORKER_COUNT,
        .registry_allowlist = &.{},
        .cp_deadlines = .{},
        .alloc = alloc,
    };
}

pub fn deinit(self: Config) void {
    self.alloc.free(self.control_plane_url);
    self.alloc.free(self.runner_token);
    self.alloc.free(self.storage_home);
    // The policy fields are placeholders or borrowed effective copies — Config
    // owns none of them.
}

/// Fail loud when `AGENTSFLEET_RUNNER_TOKEN` is not a `agt_r` runner token — a stale
/// `agt_t` from the pre-Option-B bootstrap would otherwise loop on 401s with
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
const common_constants = @import("common");
const call_deadline = @import("call_deadline");
const network = @import("../network/Policy.zig");

/// Environment variable names — single-sourced (RULE UFS). This trio is the
/// runner's COMPLETE environment surface; everything else the daemon obeys is
/// assigned by the control plane and delivered with its identity.
pub const ENV_AGENTSFLEET_API_URL = "AGENTSFLEET_API_URL";
pub const ENV_AGENTSFLEET_RUNNER_TOKEN = "AGENTSFLEET_RUNNER_TOKEN";
pub const ENV_RUNNER_STORAGE_HOME = "RUNNER_STORAGE_HOME";

const DEFAULT_STORAGE_HOME = "/tmp/agentsfleet-runner";

test "assertRunnerTokenPrefix accepts agt_r tokens, rejects everything else" {
    try assertRunnerTokenPrefix("agt_r" ++ "a" ** 64);
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("agt_tdeadbeef"));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix(""));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("agt_"));
}

test "load reads only the bootstrap trio; the environment cannot set policy" {
    const alloc = std.testing.allocator;
    var env_map = try common_constants.env.fromPairs(alloc, &.{
        .{ ENV_AGENTSFLEET_API_URL, "http://127.0.0.1:8080" },
        .{ ENV_AGENTSFLEET_RUNNER_TOKEN, "agt_r" ++ "b" ** 64 },
        .{ ENV_RUNNER_STORAGE_HOME, "/var/lib/agentsfleet-test" },
        // Decoy variables: whatever else the environment carries, policy stays
        // at its fail-closed placeholders — there is no name left that sets it.
        .{ "RUNNER_TOTALLY_UNRELATED", "landlock_full" },
        .{ "RUNNER_LEGACY_DECOY", "allow_all" },
    });
    defer env_map.deinit();

    const cfg = try Config.load(&env_map, alloc);
    defer cfg.deinit();
    try std.testing.expectEqualStrings("/var/lib/agentsfleet-test", cfg.storage_home);
    try std.testing.expectEqual(contract.protocol.SandboxTier.dev_none, cfg.sandbox_tier);
    try std.testing.expectEqual(network.FAIL_CLOSED_DEFAULT, cfg.network_policy);
    try std.testing.expectEqual(contract.protocol.DEFAULT_WORKER_COUNT, cfg.worker_count);
    try std.testing.expectEqual(@as(usize, 0), cfg.registry_allowlist.len);
    const defaults = call_deadline.Deadlines{};
    try std.testing.expectEqual(defaults.default_ms, cfg.cp_deadlines.default_ms);
    try std.testing.expectEqual(defaults.renew_ms, cfg.cp_deadlines.renew_ms);
}

test "storage home defaults when unset and honours the env when set" {
    const alloc = std.testing.allocator;
    var env_map = try common_constants.env.fromPairs(alloc, &.{
        .{ ENV_AGENTSFLEET_API_URL, "http://127.0.0.1:8080" },
        .{ ENV_AGENTSFLEET_RUNNER_TOKEN, "agt_r" ++ "c" ** 64 },
    });
    defer env_map.deinit();

    const cfg = try Config.load(&env_map, alloc);
    defer cfg.deinit();
    try std.testing.expectEqualStrings(DEFAULT_STORAGE_HOME, cfg.storage_home);
}
