// Anomaly detection: Redis-backed sliding-window counters per (zombie_id, tool, action).
// Runs BEFORE gate evaluation as a fast-path circuit breaker against runaway loops.

const std = @import("std");
const queue_redis = @import("../queue/redis_client.zig");
const ec = @import("../errors/error_registry.zig");
const config_gates = @import("config_gates.zig");

const log = std.log.scoped(.approval_gate_anomaly);

pub const AnomalyResult = enum { normal, auto_kill };

/// Check anomaly counters. Runs BEFORE gate evaluation (fast path).
/// Uses Redis INCR + EXPIRE for sliding window per (zombie_id, tool, action).
pub fn checkAnomaly(
    redis: *queue_redis.Client,
    zombie_id: []const u8,
    tool: []const u8,
    action: []const u8,
    rules: []const config_gates.AnomalyRule,
) AnomalyResult {
    for (rules) |rule| {
        switch (rule.pattern) {
            .same_action => {},
        }
        const count = incrAnomalyCounter(redis, zombie_id, tool, action, rule.threshold_window_s) catch {
            // Redis unavailable — fail open for anomaly detection (the approval
            // gate itself fails closed; only the speculative anomaly check is
            // permissive on Redis outage).
            log.warn("approval_gate.anomaly_redis_fail zombie_id={s}", .{zombie_id});
            return .normal;
        };
        if (count >= rule.threshold_count) {
            log.err("approval_gate.anomaly_auto_kill zombie_id={s} tool={s} action={s} count={d} threshold={d}", .{
                zombie_id, tool, action, count, rule.threshold_count,
            });
            return .auto_kill;
        }
    }
    return .normal;
}

// Atomic INCR + first-time-EXPIRE in a single EVAL. Two-command INCR
// followed by EXPIRE leaves a window where a Redis crash between commands
// strands the just-created key with no TTL — every subsequent call sees
// count > 1 and skips the EXPIRE branch, so the counter lives forever and
// eventually triggers an indefinite auto-kill once it crosses threshold.
const ANOMALY_INCR_SCRIPT =
    \\local v = redis.call('INCR', KEYS[1])
    \\if v == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
    \\return v
;

fn incrAnomalyCounter(
    redis: *queue_redis.Client,
    zombie_id: []const u8,
    tool: []const u8,
    action: []const u8,
    window_s: u32,
) !u32 {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}:{s}", .{
        ec.GATE_ANOMALY_KEY_PREFIX, zombie_id, tool, action,
    }) catch return error.BufferOverflow;

    var ttl_buf: [16]u8 = undefined;
    const ttl_str = std.fmt.bufPrint(&ttl_buf, "{d}", .{window_s}) catch return error.BufferOverflow;

    var resp = try redis.command(&.{ "EVAL", ANOMALY_INCR_SCRIPT, "1", key, ttl_str });
    defer resp.deinit(redis.alloc);
    return switch (resp) {
        .integer => |n| if (n > 0) @intCast(n) else 1,
        else => error.RedisCommandError,
    };
}

