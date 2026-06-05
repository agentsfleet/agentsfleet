pub const RedisRole = enum {
    default,
    api,
};

pub fn roleEnvVarName(role: RedisRole) []const u8 {
    return switch (role) {
        .api => "REDIS_URL_API",
        .default => "REDIS_URL",
    };
}
