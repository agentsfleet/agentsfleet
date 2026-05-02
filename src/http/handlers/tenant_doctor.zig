//! GET /v1/tenants/me/diagnostics — tenant-scoped doctor surface.
//!
//! Returns the resolver's view of the tenant's provider posture as a
//! `tenant_provider` block. The api_key is never serialised. User-fixable
//! BYOK errors (credential row missing, malformed credential JSON) surface
//! as an `error_label` field rather than a 5xx so the client doctor can
//! report them as actionable findings. Operator-side errors
//! (PlatformKeyMissing) still 5xx.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");

const Hx = hx_mod.Hx;

const log = std.log.scoped(.http_tenant_doctor);

pub fn innerGetTenantDoctor(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var block = tenant_provider.describeForDoctor(hx.alloc, conn, tenant_id) catch |err| switch (err) {
        tenant_provider.ResolveError.PlatformKeyMissing => {
            log.err("doctor.platform_key_missing tenant_id={s}", .{tenant_id});
            common.internalOperationError(hx.res, "Platform LLM key not configured — operator action required", hx.req_id);
            return;
        },
        tenant_provider.ResolveError.TenantHasNoWorkspace => {
            log.err("doctor.tenant_no_workspace tenant_id={s}", .{tenant_id});
            common.internalOperationError(hx.res, "Tenant has no primary workspace — bootstrap invariant violated", hx.req_id);
            return;
        },
        else => {
            log.err("doctor.resolve_failed tenant_id={s} err={s}", .{ tenant_id, @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };
    defer block.deinit(hx.alloc);

    hx.ok(.ok, .{
        .tenant_provider = .{
            .mode = block.mode.label(),
            .provider = block.provider,
            .model = block.model,
            .context_cap_tokens = block.context_cap_tokens,
            .credential_ref = block.credential_ref,
            .error_label = block.error_label,
        },
    });
}
