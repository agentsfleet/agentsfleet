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

    return Config{
        .control_plane_url = url,
        .runner_token = token,
        .host_id = host_id,
        .sandbox_tier = tier,
        .workspace_base = workspace_base,
        .network_policy = network.policyFromMap(env_map),
        .alloc = alloc,
    };
}

pub fn deinit(self: Config) void {
    self.alloc.free(self.control_plane_url);
    self.alloc.free(self.runner_token);
    self.alloc.free(self.host_id);
    self.alloc.free(self.sandbox_tier);
    self.alloc.free(self.workspace_base);
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

/// Environment variable names — single-sourced (RULE UFS).
pub const ENV_ZOMBIE_API_URL = "ZOMBIE_API_URL";
pub const ENV_ZOMBIE_RUNNER_TOKEN = "ZOMBIE_RUNNER_TOKEN";
/// Platform-admin Clerk JWT the operator-run `register` subcommand authenticates
/// with (env-preferred over `--token`, same precedence as `zombiectl` — RULE
/// UFS). NOT read by the daemon: the daemon's identity is the `zrn_` above.
pub const ENV_ZOMBIE_TOKEN = "ZOMBIE_TOKEN";
pub const ENV_RUNNER_HOST_ID = "RUNNER_HOST_ID";
pub const ENV_RUNNER_SANDBOX_TIER = "RUNNER_SANDBOX_TIER";
pub const ENV_RUNNER_WORKSPACE_BASE = "RUNNER_WORKSPACE_BASE";

// Derived from the SandboxTier enum (RULE UFS: single source). dev_none is the
// only tier that runs without isolation — dev default; prod must override.
const DEFAULT_SANDBOX_TIER = @tagName(contract.protocol.SandboxTier.dev_none);
const DEFAULT_WORKSPACE_BASE = "/tmp/zombie-runner";

test "assertRunnerTokenPrefix accepts zrn_ tokens, rejects everything else" {
    try assertRunnerTokenPrefix("zrn_" ++ "a" ** 64);
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("zmb_t_deadbeef"));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix(""));
    try std.testing.expectError(ConfigError.InvalidRunnerToken, assertRunnerTokenPrefix("zrn"));
}
