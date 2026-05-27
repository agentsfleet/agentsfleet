const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_client = @import("redis_client.zig");
const redis_subscriber = @import("redis_subscriber.zig");

pub const RedisRole = redis_types.RedisRole;
pub const roleEnvVarName = redis_types.roleEnvVarName;
pub const Client = redis_client.Client;
pub const makeConsumerId = redis_client.makeConsumerId;
pub const Subscriber = redis_subscriber;
pub const SubscriberMessage = redis_subscriber.Message;
pub const SubscriberInitOptions = redis_subscriber.InitOptions;
pub const ClientInitOptions = redis_client.InitOptions;
pub const REDIS_REQUEST_TIMEOUT_MS_ENV = redis_client.REDIS_REQUEST_TIMEOUT_MS_ENV;
pub const REDIS_REQUEST_TIMEOUT_MS_DEFAULT = redis_client.REDIS_REQUEST_TIMEOUT_MS_DEFAULT;
pub const parseRequestTimeoutMs = redis_client.parseRequestTimeoutMs;
pub const ParseRequestTimeoutError = redis_client.ParseRequestTimeoutError;

pub const testing = struct {
    pub const parseRedisUrl = redis_config.parseRedisUrl;
    pub const loadCaBundle = redis_config.loadCaBundle;
    pub const connectFromUrl = redis_client.Client.connectFromUrl;
    pub const deinitConfig = redis_config.deinitConfig;
};

test {
    _ = @import("redis_test.zig");
    _ = @import("redis_protocol.zig");
}
