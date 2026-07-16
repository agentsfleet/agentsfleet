const std = @import("std");
const common = @import("common");
const env_resolve = @import("../config/env_resolve.zig");
const error_codes = @import("../errors/error_registry.zig");
const queue_redis = @import("../queue/redis.zig");
const logging = @import("log");

const log = logging.scoped(.agentsfleetd);
const S_STARTUP_ENV_CHECK_FAILED = "startup.env_check_failed";

/// Boot-path read of the request-path Redis read-timeout knob.
/// Absent env uses the default; malformed input fails startup loudly.
pub fn read(env_map: *const common.env.Map, alloc: std.mem.Allocator) u32 {
    const raw = env_resolve.config(env_map, alloc, queue_redis.REDIS_REQUEST_TIMEOUT_MS_ENV) orelse
        return queue_redis.REDIS_REQUEST_TIMEOUT_MS_DEFAULT;
    defer alloc.free(raw);
    return queue_redis.parseRequestTimeoutMs(raw) catch {
        log.err(S_STARTUP_ENV_CHECK_FAILED, .{
            .error_code = error_codes.ERR_STARTUP_ENV_CHECK,
            .err = queue_redis.REDIS_REQUEST_TIMEOUT_MS_ENV ++ " must parse as a non-negative integer (ms)",
        });
        std.process.exit(1);
    };
}
