//! Lease execution-policy assembly and credential classification.

const std = @import("std");
const logging = @import("log");

const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const FleetSession = @import("fleet_session.zig");
const secrets_resolve = @import("secrets_resolve.zig");
const grant_lookup = @import("../state/integration_grant_lookup.zig");
const integration = @import("../credentials/integration.zig");
const service_endpoint = @import("service_endpoint.zig");
const service_repository = @import("service_repository.zig");
const context_resolve = @import("context_resolve.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const execution_policy = @import("contract").execution_policy;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_lease);

const ParkedOnGrant = struct {
    credential: []const u8,
    service: []const u8,
};

pub const Outcome = union(enum) {
    ready: execution_policy.ExecutionPolicy,
    parked: ParkedOnGrant,
};

const ClassifiedCredentials = union(enum) {
    ready: struct {
        secrets_map: ?std.json.Value,
        mintable: []const execution_policy.Mintable,
    },
    parked: ParkedOnGrant,
};

pub fn resolve(
    hx: Hx,
    session: *FleetSession,
    resolved: ?tenant_provider.ResolvedProvider,
    entries: ?[]secrets_resolve.ResolvedSecret,
    approved_services: []const []const u8,
) Outcome {
    const budget = context_resolve.resolveContextBudget(
        session.config.context,
        session.config.model,
        if (resolved) |provider| provider.context_cap_tokens else 0,
        if (resolved) |provider| provider.model else "",
    );
    const classified = switch (classifyCredentials(hx.alloc, entries, approved_services)) {
        .parked => |parked| return .{ .parked = parked },
        .ready => |ready| ready,
    };
    const endpoint = service_endpoint.customEndpoint(hx.alloc, resolved);
    return .{ .ready = .{
        .network_policy = .{
            .allow = if (session.config.network) |network| network.allow else &.{},
            .read_only = if (session.config.network) |network| network.read_only else false,
            .read_post_paths = if (session.config.network) |network| network.read_post_paths else &.{},
        },
        .secrets_map = classified.secrets_map,
        .mintable = classified.mintable,
        .context = budget,
        .provider = endpoint.provider,
        .api_key = if (resolved) |provider| provider.api_key else "",
        .inference_host = endpoint.inference_host,
        .base_url = endpoint.base_url,
        .repository_binding = service_repository.wireRepositoryBinding(session.config.repository_binding),
    } };
}

fn classifyCredentials(
    alloc: std.mem.Allocator,
    entries: ?[]secrets_resolve.ResolvedSecret,
    approved_services: []const []const u8,
) ClassifiedCredentials {
    const list = entries orelse return .{ .ready = .{ .secrets_map = null, .mintable = &.{} } };
    var object: std.json.ObjectMap = .empty;
    var mintables: std.ArrayList(execution_policy.Mintable) = .empty;
    for (list) |entry| {
        if (secrets_resolve.mintableId(entry.parsed.value)) |id| {
            const service = integration.toString(id);
            if (!grant_lookup.contains(approved_services, service)) {
                return .{ .parked = .{ .credential = entry.name, .service = service } };
            }
            mintables.append(alloc, .{ .name = entry.name, .integration = service }) catch |err|
                log.warn("lease_secret_mintable_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        } else {
            object.put(alloc, entry.name, entry.parsed.value) catch |err|
                log.warn("lease_secret_put_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        }
    }
    return .{ .ready = .{
        .secrets_map = if (object.count() > 0) .{ .object = object } else null,
        .mintable = mintables.toOwnedSlice(alloc) catch &.{},
    } };
}

test {
    _ = service_endpoint;
    _ = service_repository;
}
