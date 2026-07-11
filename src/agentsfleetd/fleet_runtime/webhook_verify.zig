// Webhook signature verification — config-driven, multi-provider.
//
// Each provider is a VerifyConfig entry. Adding a new provider = one new const.
// No switch statements, no per-provider functions.
// Uses constant-time comparison to prevent timing side-channels (RULE CTM).

const std = @import("std");
const ec = @import("../errors/error_registry.zig");
const github_app = @import("webhook/normalizer/github_app.zig");

const GITHUB_APP_IDENTITY = "github-app";

pub const VerifyConfig = struct {
    name: []const u8,
    sig_header: []const u8,
    ts_header: ?[]const u8 = null,
    prefix: []const u8,
    hmac_version: []const u8 = "",
    includes_timestamp: bool = false,
    max_ts_drift_seconds: i64 = ec.SLACK_MAX_TS_DRIFT_SECONDS,
    ingress: ?IngressConfig = null,
};

/// Provider-owned metadata consumed by the generic App ingress. JSON paths
/// are key-only traversals over the verified payload.
pub const IngressConfig = struct {
    platform_secret_key: []const u8,
    platform_secret_field: []const u8,
    routing_key_path: []const []const u8,
    repository_path: []const []const u8,
    event_header: []const u8,
    delivery_header: []const u8,
    dedup_namespace: []const u8,
    actor: []const u8,
    normalize: *const fn (std.mem.Allocator, []const u8, std.json.ObjectMap, i64) anyerror!?[]u8,
};

// ── Provider configs ──────────────────────────────────────────────────────

pub const SLACK = VerifyConfig{
    .name = "slack",
    .sig_header = ec.SLACK_SIG_HEADER,
    .ts_header = ec.SLACK_TS_HEADER,
    .prefix = "v0=",
    .hmac_version = ec.SLACK_SIG_VERSION,
    .includes_timestamp = true,
};

pub const GITHUB = VerifyConfig{
    .name = "github",
    .sig_header = "x-hub-signature-256",
    .prefix = "sha256=",
    .ingress = .{
        .platform_secret_key = GITHUB_APP_IDENTITY,
        .platform_secret_field = "webhook_secret",
        .routing_key_path = &.{ "installation", "id" },
        .repository_path = &.{ "repository", "full_name" },
        .event_header = "x-github-event",
        .delivery_header = "x-github-delivery",
        .dedup_namespace = "gh",
        .actor = GITHUB_APP_IDENTITY,
        .normalize = github_app.normalizeForIngress,
    },
};

pub const LINEAR = VerifyConfig{
    .name = "linear",
    .sig_header = "linear-signature",
    .prefix = "",
};

// ── Provider registry ─────────────────────────────────────────────────
// Comptime array of all known HMAC-SHA256 providers. Adding a new
// provider = one new const + one new entry here.

pub const PROVIDER_REGISTRY: []const VerifyConfig = &.{ SLACK, GITHUB, LINEAR };

// Comptime invariants: unique name, unique sig_header, non-empty name + sig_header.
comptime {
    for (PROVIDER_REGISTRY, 0..) |a, i| {
        if (a.name.len == 0) @compileError("VerifyConfig name must be non-empty");
        if (a.sig_header.len == 0) @compileError("VerifyConfig sig_header must be non-empty");
        if (a.ingress) |ingress| {
            if (ingress.routing_key_path.len == 0 or ingress.repository_path.len == 0)
                @compileError("IngressConfig JSON paths must be non-empty");
            if (ingress.event_header.len == 0 or ingress.delivery_header.len == 0)
                @compileError("IngressConfig event headers must be non-empty");
        }
        for (PROVIDER_REGISTRY[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a.sig_header, b.sig_header))
                @compileError("Duplicate sig_header in PROVIDER_REGISTRY: " ++ a.sig_header);
            if (std.mem.eql(u8, a.name, b.name))
                @compileError("Duplicate name in PROVIDER_REGISTRY: " ++ a.name);
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────

/// Match a provider by a `triggers[].source` value (case-insensitive),
/// falling back to request-header presence. `headers` must expose
/// `header(name) ?[]const u8`; pass `NoHeaders{}` at config-parse time
/// when no request exists.
pub fn detectProvider(source: []const u8, headers: anytype) ?VerifyConfig {
    if (source.len > 0) {
        for (PROVIDER_REGISTRY) |cfg| {
            if (std.ascii.eqlIgnoreCase(source, cfg.name)) return cfg;
        }
    }
    for (PROVIDER_REGISTRY) |cfg| {
        if (headers.header(cfg.sig_header) != null) return cfg;
    }
    return null;
}

pub const NoHeaders = struct {
    pub fn header(_: NoHeaders, _: []const u8) ?[]const u8 {
        return null;
    }
};
