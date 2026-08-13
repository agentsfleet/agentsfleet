//! Frozen version-one lease response and conversion from the current shape.

const std = @import("std");
const EventEnvelope = @import("event_envelope.zig");
const current_policy = @import("execution_policy.zig");

pub const SecretDelivery = enum { @"inline", scoped, proxy };

pub const NetworkPolicy = struct {
    allow: []const []const u8 = &.{},
};

pub const RepositoryBinding = struct {
    repositories: []const []const u8,
    access: current_policy.RepositoryAccess,
};

pub const ExecutionPolicy = struct {
    network_policy: NetworkPolicy = .{},
    tools: []const []const u8 = &.{},
    secrets_map: ?std.json.Value = null,
    mintable: []const current_policy.Mintable = &.{},
    provider: []const u8 = "",
    api_key: []const u8 = "",
    inference_host: []const u8 = "",
    base_url: ?[]const u8 = null,
    repository_binding: ?RepositoryBinding = null,
    context: current_policy.ContextBudget = .{},
};

pub const BundleManifest = struct {
    content_hash: []const u8,
};

pub const LeasePayload = struct {
    lease_id: []const u8,
    fencing_token: u64,
    lease_expires_at: i64,
    secret_delivery: SecretDelivery,
    event: EventEnvelope,
    policy: ExecutionPolicy,
    instructions: []const u8 = "",
    bundle: ?BundleManifest = null,
};

pub const LeaseResponse = struct {
    lease: ?LeasePayload = null,
    retry_after_ms: ?u32 = null,
};

/// Convert without allocating. Every slice remains borrowed from the current
/// response and is serialized synchronously by the handler.
pub fn fromCurrent(response: anytype) LeaseResponse {
    const lease = response.lease orelse return .{ .retry_after_ms = response.retry_after_ms };
    const binding: ?RepositoryBinding = if (lease.policy.repository_binding) |value| .{
        .repositories = value.repositories,
        .access = value.access,
    } else null;
    return .{ .lease = .{
        .lease_id = lease.lease_id,
        .fencing_token = lease.fencing_token,
        .lease_expires_at = lease.lease_expires_at,
        .secret_delivery = switch (lease.secret_delivery) {
            .@"inline" => .@"inline",
            .scoped => .scoped,
            .proxy => .proxy,
        },
        .event = lease.event,
        .policy = .{
            .network_policy = .{ .allow = lease.policy.network_policy.allow },
            .tools = lease.policy.tools,
            .secrets_map = lease.policy.secrets_map,
            .mintable = lease.policy.mintable,
            .provider = lease.policy.provider,
            .api_key = lease.policy.api_key,
            .inference_host = lease.policy.inference_host,
            .base_url = lease.policy.base_url,
            .repository_binding = binding,
            .context = lease.policy.context,
        },
        .instructions = lease.instructions,
        .bundle = if (lease.bundle) |value| .{ .content_hash = value.content_hash } else null,
    } };
}

test "version-one response omits version-two policy fields" {
    const Current = struct {
        lease: ?struct {
            lease_id: []const u8,
            fencing_token: u64,
            lease_expires_at: i64,
            secret_delivery: SecretDelivery,
            event: EventEnvelope,
            policy: current_policy.ExecutionPolicy,
            instructions: []const u8,
            bundle: ?BundleManifest,
        },
        retry_after_ms: ?u32,
    };
    const current = Current{ .lease = .{
        .lease_id = "lease",
        .fencing_token = 1,
        .lease_expires_at = 2,
        .secret_delivery = .@"inline",
        .event = .{ .event_id = "event", .fleet_id = "fleet", .workspace_id = "workspace", .actor = "actor", .event_type = .webhook, .request_json = "{}", .created_at = 1 },
        .policy = .{
            .network_policy = .{ .allow = &.{"api.example"}, .read_only = true },
            .http_origin_policies = &.{.{ .host = "api.example" }},
            .repository_binding = .{ .repositories = &.{"acme/repo"}, .access = .read, .base_branch = "main" },
        },
        .instructions = "run",
        .bundle = null,
    }, .retry_after_ms = null };
    const json = try std.json.Stringify.valueAlloc(std.testing.allocator, fromCurrent(current), .{});
    defer std.testing.allocator.free(json);
    try std.testing.expect(std.mem.indexOf(u8, json, "read_only") == null);
    try std.testing.expect(std.mem.indexOf(u8, json, "http_origin_policies") == null);
    try std.testing.expect(std.mem.indexOf(u8, json, "base_branch") == null);
    const strict = try std.json.parseFromSlice(LeaseResponse, std.testing.allocator, json, .{});
    defer strict.deinit();
    try std.testing.expect(strict.value.lease != null);

    const version_two_json = try std.json.Stringify.valueAlloc(std.testing.allocator, current, .{});
    defer std.testing.allocator.free(version_two_json);
    try std.testing.expectError(
        error.UnknownField,
        std.json.parseFromSlice(LeaseResponse, std.testing.allocator, version_two_json, .{}),
    );
}
