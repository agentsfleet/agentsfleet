//! GitHub connector install-state — pins the shared connector-state mechanism
//! (`connectors/state.zig`) to GitHub's domain + nonce namespace. The logic and
//! its unit tests live in the shared module; this wrapper only binds the GitHub
//! `Config` so `callback.zig`/`connect.zig` keep their existing `state.mint` /
//! `state.verifyConsume` call sites unchanged.

const std = @import("std");
const queue_redis = @import("../../../../queue/redis.zig");
const connector_state = @import("../state.zig");

const CFG = connector_state.Config{
    .domain_prefix = "ghconnect:v1:",
    .nonce_prefix = "connect:gh:nonce:",
};

pub fn mint(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    secret: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) ![]const u8 {
    return connector_state.mint(alloc, queue, CFG, secret, workspace_id, now_ms);
}

pub fn verifyConsume(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    secret: []const u8,
    state: []const u8,
    now_ms: i64,
) ?[]const u8 {
    return connector_state.verifyConsume(alloc, queue, CFG, secret, state, now_ms);
}
