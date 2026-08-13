//! GitHub production-result persistence and exact repair reconciliation.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const repair_evidence = @import("../../../../state/repair_evidence.zig");
const metrics = @import("../../../../observability/metrics_repair_verification.zig");
const deployment = @import("deployment_result.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_webhook_github);
const GITHUB_PROVIDER = "github";
const FORMAT_INTEGER = "{d}";

/// Handle every signed deployment-status delivery before generic Fleet routing.
/// Raw provider events cannot wake the verifier; only the due dispatcher emits
/// the proof-qualified synthetic event after durable exact-match correlation.
pub fn intercept(hx: Hx, conn: *pg.Conn, root: std.json.ObjectMap, workspace_id: []const u8, routed_repository: []const u8) void {
    const normalized = deployment.normalize(root);
    const production = switch (normalized) {
        .ignored => |reason| {
            metrics.incProviderResult(.ignored_normalization);
            log.info("repair_production_result_ignored", .{ .workspace_id = workspace_id, .reason = reason });
            hx.ok(.ok, .{ .ignored = reason });
            return;
        },
        .production => |value| value,
    };
    if (!std.ascii.eqlIgnoreCase(production.repository, routed_repository)) {
        metrics.incProviderResult(.ignored_repository);
        log.warn("repair_production_result_refused", .{ .error_code = ec.ERR_REPAIR_PROVENANCE_REFUSED, .workspace_id = workspace_id, .repository = routed_repository });
        hx.ok(.ok, .{ .ignored = "repository_mismatch" });
        return;
    }
    var deployment_id_buf: [24]u8 = undefined;
    const deployment_id = std.fmt.bufPrint(&deployment_id_buf, FORMAT_INTEGER, .{production.provider_deployment_id}) catch return hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Could not format deployment identifier");
    var status_id_buf: [24]u8 = undefined;
    const status_id = std.fmt.bufPrint(&status_id_buf, FORMAT_INTEGER, .{production.provider_status_id}) catch return hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Could not format deployment status identifier");
    const arrival = repair_evidence.recordProduction(hx.alloc, conn, .{
        .workspace_id = workspace_id,
        .provider = GITHUB_PROVIDER,
        .provider_deployment_id = deployment_id,
        .provider_status_id = status_id,
        .repository = production.repository,
        .environment = deployment.PRODUCTION_ENVIRONMENT,
        .commit_sha = production.commit_sha,
        .conclusion = production.conclusion,
        .completed_at = production.completed_at,
    }) catch return hx.fail(ec.ERR_INTERNAL_DB_QUERY, "Failed to retain production result");
    log.info("repair_production_result_recorded", .{
        .workspace_id = workspace_id,
        .repository = production.repository,
        .provider_deployment_id = deployment_id,
        .provider_status_id = status_id,
        .commit = production.commit_sha[0..@min(production.commit_sha.len, 12)],
        .replayed = arrival.outcome == .replayed,
        .verification_attempts = arrival.verification_attempts,
    });
    hx.ok(.accepted, .{ .status = "production_result_recorded", .verification_attempts = arrival.verification_attempts });
}
