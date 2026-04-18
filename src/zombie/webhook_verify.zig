// Webhook signature verification — config-driven, multi-provider.
//
// Each provider is a VerifyConfig entry. Adding a new provider = one new const.
// No switch statements, no per-provider functions.
// Uses constant-time comparison to prevent timing side-channels (RULE CTM).

const std = @import("std");
const hs = @import("hmac_sig");
const ec = @import("../errors/error_registry.zig");

pub const VerifyConfig = struct {
    name: []const u8,
    sig_header: []const u8,
    ts_header: ?[]const u8 = null,
    prefix: []const u8,
    hmac_version: []const u8 = "",
    includes_timestamp: bool = false,
    max_ts_drift_seconds: i64 = ec.SLACK_MAX_TS_DRIFT_SECONDS,
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
        for (PROVIDER_REGISTRY[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a.sig_header, b.sig_header))
                @compileError("Duplicate sig_header in PROVIDER_REGISTRY: " ++ a.sig_header);
            if (std.mem.eql(u8, a.name, b.name))
                @compileError("Duplicate name in PROVIDER_REGISTRY: " ++ a.name);
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────

/// Match a provider by `trigger.source` (case-insensitive), falling back to
/// request-header presence. `headers` must expose `header(name) ?[]const u8`;
/// pass `NoHeaders{}` at config-parse time when no request exists.
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

/// Verify a webhook signature using the given config.
/// Returns true if the signature is valid.
pub fn verifySignature(
    cfg: VerifyConfig,
    secret: []const u8,
    timestamp: ?[]const u8,
    body: []const u8,
    signature: []const u8,
) bool {
    if (!std.mem.startsWith(u8, signature, cfg.prefix)) return false;
    const expected = hs.hexDecode32(signature[cfg.prefix.len..]) orelse return false;

    const mac = if (cfg.includes_timestamp) blk: {
        const ts = timestamp orelse return false;
        break :blk hs.computeMac(secret, &.{ cfg.hmac_version, ":", ts, ":", body });
    } else hs.computeMac(secret, &.{body});

    return hs.constantTimeEql(&mac, &expected);
}

pub const isTimestampFresh = hs.isTimestampFresh;

