//! Runner daemon startup configuration — read once from the environment at
//! launch, before any control-plane contact.
//!
//! Env var naming follows the ZOMBIE_ namespace convention used by zombied; the
//! RUNNER_ prefix scopes variables that are runner-only and have no counterpart
//! in zombied's config. All vars are required unless a default is documented.
//!
//! This struct is datastore-free (string slices only) so it links cleanly into
//! the runner build graph, which deliberately omits pg/redis.

const std = @import("std");
const Allocator = std.mem.Allocator;
const contract = @import("contract");

/// Environment variable names — single-sourced (RULE UFS).
pub const ENV_ZOMBIE_API_URL = "ZOMBIE_API_URL";
pub const ENV_ZOMBIE_RUNNER_TOKEN = "ZOMBIE_RUNNER_TOKEN";
pub const ENV_RUNNER_HOST_ID = "RUNNER_HOST_ID";
pub const ENV_RUNNER_SANDBOX_TIER = "RUNNER_SANDBOX_TIER";
pub const ENV_RUNNER_LABELS = "RUNNER_LABELS";
pub const ENV_RUNNER_WORKSPACE_BASE = "RUNNER_WORKSPACE_BASE";

// Derived from the SandboxTier enum (RULE UFS: single source). dev_none is the
// only tier that runs without isolation — dev default; prod must override.
const DEFAULT_SANDBOX_TIER = @tagName(contract.protocol.SandboxTier.dev_none);
const DEFAULT_WORKSPACE_BASE = "/tmp/zombie-runner";

pub const ConfigError = error{ MissingEnvVar, OutOfMemory };

/// Startup configuration for the runner daemon. All slices are owned by the
/// allocator passed to `load()`; call `deinit()` when done.
pub const Config = struct {
    /// Base URL of the zombied control plane, e.g. `http://127.0.0.1:8080`.
    control_plane_url: []const u8,
    /// Provisioner credential (`zmb_t_` api_key or Clerk JSON Web Token) that
    /// authorizes POST /v1/runners. Consumed once at register(); never logged.
    register_token: []const u8,
    /// Stable machine identifier reported to the control plane at enrollment.
    host_id: []const u8,
    /// Self-reported isolation tier (stored as telemetry; placement uses
    /// operator-assigned trust, not this claim). Defaults to `dev_none`.
    sandbox_tier: []const u8,
    /// Comma-separated label string forwarded to the control plane verbatim.
    labels: []const []const u8,
    /// Base directory under which per-lease workspace subdirs are created.
    workspace_base: []const u8,

    alloc: Allocator,

    /// Read configuration from the process environment. Returns
    /// `ConfigError.MissingEnvVar` for required vars that are absent.
    pub fn load(alloc: Allocator) ConfigError!Config {
        const url = getRequired(alloc, ENV_ZOMBIE_API_URL) catch
            return ConfigError.MissingEnvVar;
        errdefer alloc.free(url);

        const token = getRequired(alloc, ENV_ZOMBIE_RUNNER_TOKEN) catch
            return ConfigError.MissingEnvVar;
        errdefer alloc.free(token);

        const host_id = getRequired(alloc, ENV_RUNNER_HOST_ID) catch
            return ConfigError.MissingEnvVar;
        errdefer alloc.free(host_id);

        const tier = std.process.getEnvVarOwned(alloc, ENV_RUNNER_SANDBOX_TIER) catch
            alloc.dupe(u8, DEFAULT_SANDBOX_TIER) catch return ConfigError.OutOfMemory;
        errdefer alloc.free(tier);

        const labels = parseLabels(alloc, std.process.getEnvVarOwned(alloc, ENV_RUNNER_LABELS) catch
            alloc.dupe(u8, "") catch return ConfigError.OutOfMemory) catch return ConfigError.OutOfMemory;
        errdefer freeLabels(alloc, labels);

        const workspace_base = std.process.getEnvVarOwned(alloc, ENV_RUNNER_WORKSPACE_BASE) catch
            alloc.dupe(u8, DEFAULT_WORKSPACE_BASE) catch return ConfigError.OutOfMemory;
        errdefer alloc.free(workspace_base);

        return Config{
            .control_plane_url = url,
            .register_token = token,
            .host_id = host_id,
            .sandbox_tier = tier,
            .labels = labels,
            .workspace_base = workspace_base,
            .alloc = alloc,
        };
    }

    pub fn deinit(self: Config) void {
        self.alloc.free(self.control_plane_url);
        self.alloc.free(self.register_token);
        self.alloc.free(self.host_id);
        self.alloc.free(self.sandbox_tier);
        freeLabels(self.alloc, self.labels);
        self.alloc.free(self.workspace_base);
    }
};

fn getRequired(alloc: Allocator, name: []const u8) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch error.MissingEnvVar;
}

/// Split `raw` on commas, trim whitespace, skip empty segments.
/// Frees `raw` before returning. Caller owns the returned slice via `alloc`.
fn parseLabels(alloc: Allocator, raw: []u8) ![]const []const u8 {
    defer alloc.free(raw);
    var out: std.ArrayList([]const u8) = .{};
    errdefer {
        for (out.items) |s| alloc.free(s);
        out.deinit(alloc);
    }
    var it = std.mem.splitScalar(u8, raw, ',');
    while (it.next()) |part| {
        const trimmed = std.mem.trim(u8, part, " \t\r\n");
        if (trimmed.len == 0) continue;
        const owned = try alloc.dupe(u8, trimmed);
        try out.append(alloc, owned);
    }
    return out.toOwnedSlice(alloc);
}

fn freeLabels(alloc: Allocator, labels: []const []const u8) void {
    for (labels) |s| alloc.free(s);
    alloc.free(labels);
}
