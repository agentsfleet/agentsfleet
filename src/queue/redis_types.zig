const std = @import("std");

pub const RedisRole = enum {
    default,
    api,
    worker,
};

pub fn roleEnvVarName(role: RedisRole) []const u8 {
    return switch (role) {
        .api => "REDIS_URL_API",
        .worker => "REDIS_URL_WORKER",
        .default => "REDIS_URL",
    };
}

