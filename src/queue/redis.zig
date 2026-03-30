const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_client = @import("redis_client.zig");
const redis_pubsub = @import("redis_pubsub.zig");

pub const RedisRole = redis_types.RedisRole;
pub const roleEnvVarName = redis_types.roleEnvVarName;
pub const QueueMessage = redis_types.QueueMessage;
pub const Client = redis_client.Client;
pub const makeConsumerId = redis_client.makeConsumerId;
pub const Subscriber = redis_pubsub.Subscriber;
pub const PubSubMessage = redis_pubsub.PubSubMessage;

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
