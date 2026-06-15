const std = @import("std");
const common = @import("common");
const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_client = @import("redis_client.zig");
const redis_subscriber = @import("redis_subscriber.zig");

pub const RedisRole = redis_types.RedisRole;
pub const roleEnvVarName = redis_types.roleEnvVarName;
pub const Client = redis_client.Client;
pub const stableConsumerId = redis_client.stableConsumerId;
pub const Subscriber = redis_subscriber;
pub const REDIS_REQUEST_TIMEOUT_MS_ENV = redis_config.REDIS_REQUEST_TIMEOUT_MS_ENV;
pub const REDIS_REQUEST_TIMEOUT_MS_DEFAULT = redis_config.REDIS_REQUEST_TIMEOUT_MS_DEFAULT;
pub const parseRequestTimeoutMs = redis_config.parseRequestTimeoutMs;

pub const testing = struct {
    pub const parseRedisUrl = redis_config.parseRedisUrl;
    pub const loadCaBundle = redis_config.loadCaBundle;
    pub const deinitConfig = redis_config.deinitConfig;

    /// The broker TLS CA path the integration harness exports
    /// (`REDIS_TLS_CA_CERT_FILE`). The URL-only connect helpers below pass it so
    /// tests verify the self-signed cert the env-map prod paths already resolve.
    fn caCertFromEnv() ?[]const u8 {
        return common.env.testLiveValue(redis_config.CA_CERT_FILE_ENV);
    }

    /// Test Client over a URL with the harness CA wired in.
    pub fn connectFromUrl(io: std.Io, alloc: std.mem.Allocator, url: []const u8) !redis_client.Client {
        return redis_client.Client.connectFromUrlWithOptions(io, alloc, url, .{ .ca_cert_file = caCertFromEnv() });
    }

    /// As above, preserving caller options (read timeout); CA defaults to the harness cert.
    pub fn connectFromUrlWithOptions(io: std.Io, alloc: std.mem.Allocator, url: []const u8, options: redis_client.InitOptions) !redis_client.Client {
        var opts = options;
        if (opts.ca_cert_file == null) opts.ca_cert_file = caCertFromEnv();
        return redis_client.Client.connectFromUrlWithOptions(io, alloc, url, opts);
    }

    /// Test Subscriber over a URL with the harness CA wired in.
    pub fn subscriberFromUrl(io: std.Io, alloc: std.mem.Allocator, url: []const u8, options: redis_subscriber.InitOptions) !redis_subscriber.Subscriber {
        var opts = options;
        if (opts.ca_cert_file == null) opts.ca_cert_file = caCertFromEnv();
        return redis_subscriber.connectFromUrl(io, alloc, url, opts);
    }

    /// Parse a TLS URL into a pool `Config` with the harness CA wired in. The CA
    /// is owned by the returned Config — hand it to `Pool.init`, which takes
    /// ownership and frees it in `deinit`.
    pub fn poolConfigFromUrl(alloc: std.mem.Allocator, url: []const u8) !redis_config.Config {
        var cfg = try redis_config.parseRedisUrl(alloc, url);
        errdefer redis_config.deinitConfig(alloc, cfg);
        if (cfg.ca_cert_file == null) {
            if (caCertFromEnv()) |ca| cfg.ca_cert_file = try alloc.dupe(u8, ca);
        }
        return cfg;
    }
};

test {
    _ = @import("redis_test.zig");
    _ = @import("redis_protocol.zig");
    _ = @import("redis_agent.zig");
}
