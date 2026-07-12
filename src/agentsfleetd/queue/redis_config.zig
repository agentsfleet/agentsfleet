const std = @import("std");
const common = @import("common");
const redis_types = @import("redis_types.zig");

const EnvMap = common.env.Map;

pub const Config = struct {
    host: []const u8,
    port: u16,
    username: ?[]const u8 = null,
    password: ?[]const u8 = null,
    use_tls: bool = false,
    /// Absolute path to a custom TLS CA bundle (`REDIS_TLS_CA_CERT_FILE`),
    /// resolved once from the env snapshot at connect and owned by the Config.
    /// Null → system trust store. Zig 0.16's env is a snapshot, so this is
    /// read at connect time (not per-dial).
    ca_cert_file: ?[]const u8 = null,
};

/// Env var naming the custom TLS CA bundle path; resolved into `Config.ca_cert_file`
/// by the env-map connect paths and the test connect helpers.
pub const CA_CERT_FILE_ENV = "REDIS_TLS_CA_CERT_FILE";

pub fn deinitConfig(alloc: std.mem.Allocator, cfg: Config) void {
    alloc.free(cfg.host);
    if (cfg.username) |v| alloc.free(v);
    if (cfg.password) |v| alloc.free(v);
    if (cfg.ca_cert_file) |v| alloc.free(v);
}

pub fn loadCaBundle(io: std.Io, alloc: std.mem.Allocator, ca_file_path: ?[]const u8) !std.crypto.Certificate.Bundle {
    var ca_bundle: std.crypto.Certificate.Bundle = .empty;
    errdefer ca_bundle.deinit(alloc);

    // Zig 0.16 Bundle cert loading is io-based and timestamp-validated. Cert
    // validity (notBefore/notAfter) is wall-clock, so use `.real` — `.awake` is
    // CLOCK_MONOTONIC (seconds since boot), which reads as ≪ any epoch notBefore
    // and rejects every cert as CertificateNotYetValid.
    const now = std.Io.Timestamp.now(io, .real);
    if (ca_file_path) |path| {
        if (!std.fs.path.isAbsolute(path)) return error.RedisTlsCaFileMustBeAbsolute;
        try ca_bundle.addCertsFromFilePathAbsolute(alloc, io, now, path);
    } else {
        try ca_bundle.rescan(alloc, io, now);
    }

    return ca_bundle;
}

/// Resolve the Redis URL for `role` from the env. Caller must free the returned slice.
pub fn resolveRedisUrl(env_map: *const EnvMap, alloc: std.mem.Allocator, role: redis_types.RedisRole) ![]const u8 {
    const url = (try common.env.owned(env_map, alloc, redis_types.roleEnvVarName(role))) orelse return error.MissingRedisUrl;
    if (std.mem.trim(u8, url, " \t\r\n").len == 0) {
        alloc.free(url);
        return error.MissingRedisUrl;
    }
    return url;
}

pub fn parseRedisUrl(alloc: std.mem.Allocator, url: []const u8) !Config {
    var host_owned: ?[]u8 = null;
    var username_owned: ?[]u8 = null;
    var password_owned: ?[]u8 = null;
    var cfg = Config{ .host = "", .port = 6379 };

    errdefer {
        if (host_owned) |v| alloc.free(v);
        if (username_owned) |v| alloc.free(v);
        if (password_owned) |v| alloc.free(v);
    }

    const rest = if (std.mem.startsWith(u8, url, "redis://")) blk: {
        cfg.use_tls = false;
        break :blk url["redis://".len..];
    } else if (std.mem.startsWith(u8, url, "rediss://")) blk: {
        cfg.use_tls = true;
        break :blk url["rediss://".len..];
    } else return error.InvalidRedisUrl;

    const at_pos = std.mem.lastIndexOfScalar(u8, rest, '@');
    const hostpath = if (at_pos) |at| blk: {
        const userpass = rest[0..at];
        if (std.mem.indexOfScalar(u8, userpass, ':')) |colon| {
            if (colon > 0) username_owned = try alloc.dupe(u8, userpass[0..colon]);
            password_owned = try alloc.dupe(u8, userpass[colon + 1 ..]);
        } else {
            password_owned = try alloc.dupe(u8, userpass);
        }
        break :blk rest[at + 1 ..];
    } else rest;

    const slash_pos = std.mem.indexOfScalar(u8, hostpath, '/') orelse hostpath.len;
    const hostport = hostpath[0..slash_pos];
    if (hostport.len == 0) return error.InvalidRedisUrl;

    if (std.mem.lastIndexOfScalar(u8, hostport, ':')) |colon| {
        cfg.port = std.fmt.parseInt(u16, hostport[colon + 1 ..], 10) catch return error.InvalidRedisUrl;
        host_owned = try alloc.dupe(u8, hostport[0..colon]);
    } else {
        host_owned = try alloc.dupe(u8, hostport);
    }

    cfg.host = host_owned.?;
    cfg.username = username_owned;
    cfg.password = password_owned;
    host_owned = null;
    username_owned = null;
    password_owned = null;

    return cfg;
}

/// Boot-path env knob for the request-path read timeout. Read in `serve.zig`
/// and threaded through `Client.connectFromEnvWithOptions`; the env-name
/// string is shared with operator docs verbatim.
pub const REDIS_REQUEST_TIMEOUT_MS_ENV = "REDIS_REQUEST_TIMEOUT_MS";
pub const REDIS_REQUEST_TIMEOUT_MS_DEFAULT: u32 = 5000;

/// Typed parse error for `parseRequestTimeoutMs` — carries the env-var
/// identity so the boot-path log site stays honest about which knob misparsed.
pub const ParseRequestTimeoutError = error{InvalidRequestTimeout};

/// Parse a raw env-string into a request-timeout millisecond value. Pure
/// helper — serve.zig wraps the env-read + log-and-exit ceremony around it.
pub fn parseRequestTimeoutMs(raw: []const u8) ParseRequestTimeoutError!u32 {
    return std.fmt.parseInt(u32, raw, 10) catch error.InvalidRequestTimeout;
}
