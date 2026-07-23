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
const sql = @import("sql.zig");
const httpz = @import("httpz");
const pg = @import("pg");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const constants = @import("common");
const ec = @import("../../../errors/error_registry.zig");
const pg_query = @import("../../../db/pg_query.zig");
const vault = @import("../../../state/vault.zig");
const integration = @import("../../../credentials/integration.zig");
const CredentialBroker = @import("../../../credentials/broker.zig");
const connector_oauth_refresh = @import("../connectors/oauth_refresh.zig");
const grant_lookup = @import("../../../state/integration_grant_lookup.zig");
const logging = @import("log");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const PgQuery = pg_query.PgQuery;

const log = logging.scoped(.credential_mint);

// Detail strings shared between the early-resolve fail paths and `dispose` (the
// broker can reach the same outcome from either side), single-sourced (RULE UFS).
const S_INTEGRATION_NOT_CONNECTED = "Integration not connected for this workspace";
const S_MINT_FAILED = "Credential mint failed";
// Connector (oauth2 refresh) copy — the github integration is a GitHub App
// *installation* (its own reconnect semantics), the refresh-token connectors
// re-mint from a stored refresh token, so their failure copy is provider-neutral.
const S_CONNECTOR_RECONNECT = "Connector authorization expired — reconnect the integration";
const S_CONNECTOR_MINT_FAILED = "Connector token refresh failed";
// Grant-gate refusal (invariant: no token without an approved
// grant). The registered UZ-GRANT-001 hint carries the request-grant recovery.
const S_GRANT_REQUIRED = "No approved integration grant for this fleet and integration";
// Rotated-refresh write-back observability (RULE OBS): one event, three
// outcomes. No token bytes ever ride these lines (VLT).
const EVT_REFRESH_ROTATED = "refresh_rotated";
const S_ROTATE_PERSISTED = "persisted";
const S_ROTATE_FAILED = "failed";
const S_ROTATE_SKIPPED_STALE = "skipped_stale";

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
        common.errorResponse(hx.res, ec.ERR_CRED_BROKER_NOT_CONFIGURED, "This deployment isn't set up to mint credentials yet", hx.req_id);
        return;
    };

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = parseMintRequest(hx, raw_body) orelse return; // error already written
    defer parsed.deinit();

    var inputs = loadMintInputs(hx, runner_id, parsed.value) orelse return; // error already written
    defer inputs.handle.deinit();
    mintAndRespond(hx, broker, parsed.value, &inputs);
}

/// Parse the mint request body. Writes the typed error and returns null on any
/// failure; the caller just returns.
fn parseMintRequest(hx: Hx, raw_body: []const u8) ?std.json.Parsed(protocol.MintCredentialRequest) {
    return std.json.parseFromSlice(protocol.MintCredentialRequest, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed mint request body");
        return null;
    };
}

/// Run the broker mint and map the outcome to the wire; a rotated refresh
/// token is persisted first (non-fatal). No DB connection is held across the
/// broker's network exchange. Residual accepted race: the lease was validated
/// in `loadMintInputs`, whose conn is released before the exchange, so a lease
/// expiring mid-exchange can still receive this ≤1h token — bounded to one
/// request, and a recheck could only withhold (never un-mint) the upstream
/// credential. Tracked as a separate atomic-mint follow-up.
fn mintAndRespond(hx: Hx, broker: *CredentialBroker, mint_req: protocol.MintCredentialRequest, inputs: *MintInputs) void {
    const result = broker.mint(
        hx.alloc,
        inputs.workspace_id,
        mint_req.integration,
        inputs.handle.value,
        constants.clock.nowMillis(),
    ) catch {
        // mint() only surfaces an error on an OOM-class failure (the integration
        // failures are tagged-union variants, not errors). Treat as transient, with
        // provider-appropriate copy.
        const d = dispose(integration.idFromString(mint_req.integration), .{ .mint_failed = .transient });
        hx.fail(d.code.?, d.detail);
        return;
    };
    if (result == .ok) {
        if (result.ok.rotated_refresh_token) |rotated|
            persistRotatedRefresh(hx, mint_req.integration, inputs.workspace_id, inputs.handle.value, rotated);
    }
    respond(hx, integration.idFromString(mint_req.integration), result);
}

/// Persist a rotated refresh token back to the vaulted handle so the next cold
/// mint posts the token the provider now expects (a rotating provider has
/// already invalidated the posted one). Non-fatal by design: the mint
/// succeeded and the child holds a valid access token, so a failed persist is
/// warn-logged — the operator's breadcrumb — and costs at most one forced
/// reconnect later; it never fails the request (RULE ECL: a write-back failure
/// is not a mint failure). A persist SKIPPED because the vault row changed
/// under the exchange (an admin reconnected mid-mint) is correct behavior —
/// the rotated token belongs to the replaced grant — logged as its own branch.
fn persistRotatedRefresh(hx: Hx, integration_id: []const u8, workspace_id: []const u8, handle: std.json.Value, rotated: []const u8) void {
    const posted = postedRefreshToken(handle) orelse return; // non-refresh handle: nothing to merge against
    const persisted = writeBackRotated(hx, integration_id, workspace_id, posted, rotated) catch |err| {
        log.warn(EVT_REFRESH_ROTATED, .{ .workspace_id = workspace_id, .integration = integration_id, .outcome = S_ROTATE_FAILED, .err = @errorName(err) });
        return;
    };
    if (persisted) {
        log.debug(EVT_REFRESH_ROTATED, .{ .workspace_id = workspace_id, .integration = integration_id, .outcome = S_ROTATE_PERSISTED });
    } else {
        log.debug(EVT_REFRESH_ROTATED, .{ .workspace_id = workspace_id, .integration = integration_id, .outcome = S_ROTATE_SKIPPED_STALE });
    }
}

/// The refresh token this mint POSTED — read from the pre-exchange handle
/// snapshot; the write-back's guarded merge compares it against the row's
/// current value to detect a concurrent reconnect.
fn postedRefreshToken(handle: std.json.Value) ?[]const u8 {
    const obj = switch (handle) {
        .object => |o| o,
        else => return null,
    };
    return switch (obj.get(integration.FIELD_REFRESH_TOKEN) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

/// The fallible write-back core: re-acquire a conn (the mint inputs' conn was
/// released before the upstream exchange) and merge-store through the connect
/// callback's own store path. Returns whether the merge persisted (false =
/// dropped because the stored grant changed under the exchange).
fn writeBackRotated(hx: Hx, integration_id: []const u8, workspace_id: []const u8, posted: []const u8, rotated: []const u8) !bool {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    return connector_oauth_refresh.storeRotatedRefreshToken(hx, conn, integration_id, workspace_id, posted, rotated);
}

/// Resolve the lease's workspace+fleet (scoped to the presenting runner), gate
/// on the fleet's approved grant, then load the connected integration handle —
/// all under one connection. Writes the typed error and returns null on any
/// failure; the caller just returns.
fn loadMintInputs(hx: Hx, runner_id: []const u8, mint_req: protocol.MintCredentialRequest) ?MintInputs {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    defer hx.ctx.pool.release(conn);

    const scope = (resolveLeaseScope(hx, conn, runner_id, mint_req.lease_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, "No lease matches this lease_id for the runner");
        return null;
    };

    // Grant-gate invariant — on-demand connector mints (github/zoho/jira/linear)
    // require an approved grant; the read precedes the vault load so an ungranted
    // request never touches handle bytes. A DB failure here fails CLOSED (500, no
    // token), never open. Gating ONLY on-demand integrations mirrors the lease
    // classifier (`secrets_resolve.mintableId`) and the spec scope: a `static`
    // handle (token carried inline, never minted in production) and an unknown id
    // fall through to the existing not-connected/vault path — so a typo'd or
    // unconnected id still surfaces UZ-CRED-* / unknown_integration, not a
    // misleading grant-required for a grant that can never be requested.
    if (isOnDemand(mint_req.integration)) {
        const approved = grant_lookup.isApproved(conn, scope.fleet_id, mint_req.integration) catch {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        };
        if (!approved) {
            log.warn("credential_mint_denied", .{ .error_code = ec.ERR_GRANT_NOT_FOUND, .fleet_id = scope.fleet_id, .integration = mint_req.integration });
            hx.fail(ec.ERR_GRANT_NOT_FOUND, S_GRANT_REQUIRED);
            return null;
        }
    }

    const handle = vault.loadJson(hx.alloc, conn, scope.workspace_id, mint_req.integration) catch |err| {
        if (err == error.NotFound) {
            // No handle for this workspace → the integration was never connected.
            hx.fail(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, S_INTEGRATION_NOT_CONNECTED);
            return null;
        }
        common.internalOperationError(hx.res, "failed to load integration handle", hx.req_id);
        return null;
    };
    return .{ .workspace_id = scope.workspace_id, .handle = handle };
}

/// True when the integration mints a short-lived token on demand (github App
/// installation + the oauth2-refresh connectors) — the set the grant gate
/// covers. Unknown ids and the `static` inline-token integration are false.
fn isOnDemand(integration_id: []const u8) bool {
    const id = integration.idFromString(integration_id) orelse return false;
    return integration.mintsOnDemand(integration.REGISTRY, id);
}

/// The lease's workspace + fleet, both arena-duped (survive the conn release).
const LeaseScope = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
};

/// Resolve the lease scoped to the presenting runner (Invariant 2: the runner-id
/// scope is the ownership check) AND still live — `status = active` and unexpired.
/// A foreign, expired, or revoked `lease_id` → null → 404, so mint authority is
/// bound to the lease's lifetime, not the runner's: a cancelled/expired run, or a
/// compromised runner replaying a stale `lease_id`, cannot mint past the lease.
/// Mirrors the active-lease predicate the sibling `memory.zig` already enforces.
/// Also returns the lease's fleet id — the scope the grant-gate checks (the grant gate).
fn resolveLeaseScope(hx: Hx, conn: *pg.Conn, runner_id: []const u8, lease_id: []const u8) !?LeaseScope {
    var q = PgQuery.from(try conn.query(sql.SELECT_LEASE_SCOPE_FOR_MINT, .{ lease_id, runner_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, constants.clock.nowMillis() }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const workspace_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer hx.alloc.free(workspace_id);
    const fleet_id = try hx.alloc.dupe(u8, try row.get([]const u8, 1));
    return .{ .workspace_id = workspace_id, .fleet_id = fleet_id };
}

/// The wire disposition of a broker outcome: a `null` code means success (200 with
/// the token); a non-null code + detail is the RFC 7807 error. Pure — extracted so
/// the result→wire contract (every Failure Mode → a typed code, never a silent
/// 401) is unit-tested with no live broker or DB. The token is NEVER read here.
const Disposition = struct { code: ?[]const u8, detail: []const u8 };

fn dispose(id: ?integration.Id, result: integration.MintResult) Disposition {
    // github is a GitHub App installation (its own reconnect + mint copy); the
    // oauth2-refresh connectors (zoho/jira/linear) surface the shared connector
    // oauth-exchange code (UZ-CONN-006). unknown_integration is provider-neutral.
    const is_github = if (id) |i| i == .github else false;
    return switch (result) {
        .ok => .{ .code = null, .detail = "" },
        .unknown_integration => .{ .code = ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, .detail = S_INTEGRATION_NOT_CONNECTED },
        .reconnect_required => if (is_github)
            .{ .code = ec.ERR_GH_RECONNECT_REQUIRED, .detail = "GitHub App installation needs reconnect" }
        else
            .{ .code = ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, .detail = S_CONNECTOR_RECONNECT },
        .mint_failed => if (is_github)
            .{ .code = ec.ERR_GH_MINT_FAILED, .detail = S_MINT_FAILED }
        else
            .{ .code = ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, .detail = S_CONNECTOR_MINT_FAILED },
    };
}

/// Map the broker outcome to the response. `id` selects provider-appropriate error
/// copy. The minted token appears only in the 200 body here — never logged, never
/// echoed into a frame (VLT).
fn respond(hx: Hx, id: ?integration.Id, result: integration.MintResult) void {
    switch (result) {
        .ok => |minted| hx.okSensitive(.ok, protocol.MintCredentialResponse{
            .token = minted.token,
            .expires_at_ms = minted.expires_at_ms,
        }),
        else => {
            const d = dispose(id, result);
            hx.fail(d.code.?, d.detail);
        },
    }
}

test "isOnDemand gates the connector integrations, not static or unknown ids" {
    // The grant gate covers exactly the on-demand connector mints — static
    // (inline token) and unknown ids fall through to the existing paths.
    try std.testing.expect(isOnDemand("github"));
    try std.testing.expect(isOnDemand("zoho"));
    try std.testing.expect(isOnDemand("jira"));
    try std.testing.expect(isOnDemand("linear"));
    try std.testing.expect(!isOnDemand("static"));
    try std.testing.expect(!isOnDemand("nope")); // unknown id → not gated
    try std.testing.expect(!isOnDemand("")); // empty → not gated
}

test "dispose maps each broker outcome to its typed wire code; ok carries no error" {
    // Success → no error code (the handler writes 200 + the token instead).
    try std.testing.expect(dispose(.github, .{ .ok = .{ .token = "ghs_x", .expires_at_ms = 1 } }).code == null);
    // github (App installation) keeps its GitHub-specific reconnect/mint copy.
    try std.testing.expectEqualStrings(ec.ERR_GH_RECONNECT_REQUIRED, dispose(.github, .reconnect_required).code.?);
    try std.testing.expectEqualStrings(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, dispose(.github, .unknown_integration).code.?);
    // Both retry classes surface the same mint-failed code (the Retry tag is for
    // the broker's own metrics, not a wire distinction).
    try std.testing.expectEqualStrings(ec.ERR_GH_MINT_FAILED, dispose(.github, .{ .mint_failed = .transient }).code.?);
    try std.testing.expectEqualStrings(ec.ERR_GH_MINT_FAILED, dispose(.github, .{ .mint_failed = .permanent }).code.?);
}

test "dispose: oauth2-refresh connectors surface UZ-CONN-006, not GitHub copy" {
    // A zoho/jira/linear refresh failure must NOT tell the runner to reconnect a
    // GitHub App — both reconnect (revoked refresh) and mint_failed map to the
    // shared connector oauth-exchange code (Dimension 3.2).
    try std.testing.expectEqualStrings(ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, dispose(.zoho, .reconnect_required).code.?);
    try std.testing.expectEqualStrings(ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, dispose(.jira, .{ .mint_failed = .transient }).code.?);
    try std.testing.expectEqualStrings(ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, dispose(.linear, .reconnect_required).code.?);
    // unknown_integration stays provider-neutral regardless of id.
    try std.testing.expectEqualStrings(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED, dispose(null, .unknown_integration).code.?);
}
