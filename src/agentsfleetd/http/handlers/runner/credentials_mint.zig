//! POST /v1/runners/me/credentials/mint — on-demand credential mint (M102 §3).
//!
//! A sandboxed child asks its runner for a short-lived token at the moment a tool
//! needs it; the runner forwards the ask here over the `agt_r` plane. Two
//! invariants shape this handler:
//!   * Invariant 2 (workspace scope) — the workspace is derived from the lease
//!     server-side (`fleet.runner_leases`, scoped to the presenting runner). The
//!     request body carries no workspace, so a prompt-injected child that forges
//!     one has nothing to forge: a foreign or stale `lease_id` simply resolves to
//!     no row → 404, never another tenant's workspace.
//!   * Invariant 1/8 (key + token hygiene, VLT) — the platform App key never
//!     leaves the daemon's broker; only the minted token crosses back, and it is
//!     written into the response body alone — never a log line or a frame.
//!
//! The broker owns the mint (registry dispatch, cache, the App key); this handler
//! only resolves the lease's workspace + the connected integration handle and maps
//! the broker's tagged outcome to the wire (`UZ-CRED-*` / `UZ-GH-*`).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const constants = @import("common");
const ec = @import("../../../errors/error_registry.zig");
const pg_query = @import("../../../db/pg_query.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const vault = @import("../../../state/vault.zig");
const integration = @import("../../../credentials/integration.zig");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const PgQuery = pg_query.PgQuery;

// Detail strings shared between the early-resolve fail paths and `dispose` (the
// broker can reach the same outcome from either side), single-sourced (RULE UFS).
const S_INTEGRATION_NOT_CONNECTED = "Integration not connected for this workspace";
const S_MINT_FAILED = "Credential mint failed";

/// The lease's workspace + the connected integration handle, resolved together
/// under one DB connection so the connection is released before the broker's
/// network mint (a DB conn is never held across the upstream token exchange).
const MintInputs = struct {
    /// Arena-owned (`hx.alloc`); the broker scopes the mint + cache key to it.
    workspace_id: []const u8,
    /// The vault handle (`{integration, …}`); the broker reads its `integration`
    /// field to dispatch. Caller `.deinit()`s it after the mint.
    handle: std.json.Parsed(std.json.Value),
};

pub fn innerRunnerCredentialsMint(hx: Hx, req: *httpz.Request) void {
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const broker = hx.ctx.broker orelse {
        // The broker is a boot-wired daemon singleton; a null here is a
        // deployment misconfiguration, not a client error. Fail loud.
        common.internalOperationError(hx.res, "credential broker not configured", hx.req_id);
        return;
    };

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(protocol.MintCredentialRequest, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed mint request body");
        return;
    };
    defer parsed.deinit();
    const mint_req = parsed.value;

    var inputs = loadMintInputs(hx, runner_id, mint_req) orelse return; // error already written
    defer inputs.handle.deinit();

    // No DB connection is held here — the broker may do a network token exchange.
    //
    // Residual lease-check race (accepted; deferred to a follow-up hardening):
    // the lease is validated live in `loadMintInputs`, which then releases the
    // conn before this exchange. The exchange is non-atomic w.r.t. the lease, so
    // if the lease expires (raw TTL) or is reclaimed during the in-flight
    // exchange, a ≤1h token still returns. The window is bounded to a single
    // request (~exchange duration) and only bites at the exact expiry/kill edge;
    // the unbounded replay-past-kill hole is already closed by the live-lease
    // gate above. A recheck-after-mint would only *withhold* the token — it
    // cannot un-mint the upstream credential, which lives ≤1h regardless — so
    // the marginal value is low. Tracked as a separate atomic-mint follow-up.
    const result = broker.mint(
        hx.alloc,
        inputs.workspace_id,
        mint_req.integration,
        inputs.handle.value,
        constants.clock.nowMillis(),
    ) catch {
        // mint() only surfaces an error on an OOM-class failure (the integration
        // failures are tagged-union variants, not errors). Treat as transient.
        hx.fail(ec.ERR_GH_MINT_FAILED, S_MINT_FAILED);
        return;
    };
    respond(hx, result);
}

/// Resolve the lease's workspace (scoped to the presenting runner) + load the
/// connected integration handle, both under one connection. Writes the typed
/// error and returns null on any failure; the caller just returns.
fn loadMintInputs(hx: Hx, runner_id: []const u8, mint_req: protocol.MintCredentialRequest) ?MintInputs {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    defer hx.ctx.pool.release(conn);

    const workspace_id = (resolveLeaseWorkspace(hx, conn, runner_id, mint_req.lease_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, "No lease matches this lease_id for the runner");
        return null;
    };

    const key_name = credential_key.allocKeyName(hx.alloc, mint_req.integration) catch {
        common.internalOperationError(hx.res, "failed to build credential key", hx.req_id);
        return null;
    };
    defer hx.alloc.free(key_name);

    const handle = vault.loadJson(hx.alloc, conn, workspace_id, key_name) catch |err| {
        if (err == error.NotFound) {
            // No handle for this workspace → the integration was never connected.
            hx.fail(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, S_INTEGRATION_NOT_CONNECTED);
            return null;
        }
        common.internalOperationError(hx.res, "failed to load integration handle", hx.req_id);
        return null;
    };
    return .{ .workspace_id = workspace_id, .handle = handle };
}

/// Resolve the lease scoped to the presenting runner (Invariant 2: the runner-id
/// scope is the ownership check) AND still live — `status = active` and unexpired.
/// A foreign, expired, or revoked `lease_id` → null → 404, so mint authority is
/// bound to the lease's lifetime, not the runner's: a cancelled/expired run, or a
/// compromised runner replaying a stale `lease_id`, cannot mint past the lease.
/// Mirrors the active-lease predicate the sibling `memory.zig` already enforces.
/// Returns the workspace id arena-duped (survives the connection release).
fn resolveLeaseWorkspace(hx: Hx, conn: *pg.Conn, runner_id: []const u8, lease_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text
        \\FROM fleet.runner_leases
        \\WHERE id = $1::uuid AND runner_id = $2::uuid
        \\  AND status = $3 AND lease_expires_at > $4
    , .{ lease_id, runner_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, constants.clock.nowMillis() }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try hx.alloc.dupe(u8, try row.get([]const u8, 0));
}

/// The wire disposition of a broker outcome: a `null` code means success (200 with
/// the token); a non-null code + detail is the RFC 7807 error. Pure — extracted so
/// the result→wire contract (every Failure Mode → a typed code, never a silent
/// 401) is unit-tested with no live broker or DB. The token is NEVER read here.
const Disposition = struct { code: ?[]const u8, detail: []const u8 };

fn dispose(result: integration.MintResult) Disposition {
    return switch (result) {
        .ok => .{ .code = null, .detail = "" },
        .reconnect_required => .{ .code = ec.ERR_GH_RECONNECT_REQUIRED, .detail = "GitHub App installation needs reconnect" },
        .unknown_integration => .{ .code = ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, .detail = S_INTEGRATION_NOT_CONNECTED },
        .mint_failed => .{ .code = ec.ERR_GH_MINT_FAILED, .detail = S_MINT_FAILED },
    };
}

/// Map the broker outcome to the response. The minted token appears only in the
/// 200 body here — never logged, never echoed into a frame (VLT).
fn respond(hx: Hx, result: integration.MintResult) void {
    switch (result) {
        .ok => |minted| hx.ok(.ok, protocol.MintCredentialResponse{
            .token = minted.token,
            .expires_at_ms = minted.expires_at_ms,
        }),
        else => {
            const d = dispose(result);
            hx.fail(d.code.?, d.detail);
        },
    }
}

test "dispose maps each broker outcome to its typed wire code; ok carries no error" {
    // Success → no error code (the handler writes 200 + the token instead).
    try std.testing.expect(dispose(.{ .ok = .{ .token = "ghs_x", .expires_at_ms = 1 } }).code == null);
    // Every failure mode is a distinct typed code — never a silent 401.
    try std.testing.expectEqualStrings(ec.ERR_GH_RECONNECT_REQUIRED, dispose(.reconnect_required).code.?);
    try std.testing.expectEqualStrings(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, dispose(.unknown_integration).code.?);
    // Both retry classes surface the same mint-failed code (the Retry tag is for
    // the broker's own metrics, not a wire distinction).
    try std.testing.expectEqualStrings(ec.ERR_GH_MINT_FAILED, dispose(.{ .mint_failed = .transient }).code.?);
    try std.testing.expectEqualStrings(ec.ERR_GH_MINT_FAILED, dispose(.{ .mint_failed = .permanent }).code.?);
}
