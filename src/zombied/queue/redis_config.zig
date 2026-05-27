const std = @import("std");
const redis_types = @import("redis_types.zig");

pub const Config = struct {
    host: []const u8,
    port: u16,
    username: ?[]const u8 = null,
    password: ?[]const u8 = null,
    use_tls: bool = false,
};

pub fn deinitConfig(alloc: std.mem.Allocator, cfg: Config) void {
    alloc.free(cfg.host);
    if (cfg.username) |v| alloc.free(v);
    if (cfg.password) |v| alloc.free(v);
}

pub fn loadCaBundle(alloc: std.mem.Allocator, ca_file_path: ?[]const u8) !std.crypto.Certificate.Bundle {
    var ca_bundle: std.crypto.Certificate.Bundle = .{};
    errdefer ca_bundle.deinit(alloc);

    if (ca_file_path) |path| {
        if (!std.fs.path.isAbsolute(path)) return error.RedisTlsCaFileMustBeAbsolute;
        try ca_bundle.addCertsFromFilePathAbsolute(alloc, path);
    } else {
        try ca_bundle.rescan(alloc);
    }

    return ca_bundle;
}

pub fn resolveRedisUrl(alloc: std.mem.Allocator, role: redis_types.RedisRole) ![]u8 {
    const url = std.process.getEnvVarOwned(alloc, redis_types.roleEnvVarName(role)) catch return error.MissingRedisUrl;
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
