/// error_entries_runtime.zig — runtime / execute-path error entries.
///
/// Sibling of error_entries.zig (control-plane entries). Split for the
/// 350-line file cap. Both arrays are concatenated by error_registry.zig.
const std = @import("std");
const entries = @import("error_entries.zig");
const Entry = entries.Entry;
const ERROR_DOCS_BASE = entries.ERROR_DOCS_BASE;

const S_ALREADY_RESOLVED_USER_MSG = "Someone already resolved this. Refresh to see the outcome and who resolved it.";

fn e(
    comptime code: []const u8,
    comptime status: std.http.Status,
    comptime title: []const u8,
    comptime hint_text: []const u8,
) Entry {
    return .{
        .code = code,
        .http_status = status,
        .title = title,
        .hint = hint_text,
        .docs_uri = ERROR_DOCS_BASE ++ code,
    };
}

/// Like e(), plus a curated `user_message` — see error_entries.zig's module
/// doc for when to reach for this instead of e().
fn eu(
    comptime code: []const u8,
    comptime status: std.http.Status,
    comptime title: []const u8,
    comptime hint_text: []const u8,
    comptime user_message_text: []const u8,
) Entry {
    var entry = e(code, status, title, hint_text);
    entry.user_message = user_message_text;
    return entry;
}

pub const ENTRIES_RUNTIME = [_]Entry{
    // ── SANDBOX ──────────────────────────────────────────────────────────────
    // ── RUNNER ─────────────────────────────────────────────────────────────
    // UZ-EXEC-001 retired: no producer ever emitted it.
    // UZ-EXEC-002 retired: no producer ever emitted it.
    e("UZ-EXEC-003", .internal_server_error, "Execution timeout kill", "Execution exceeded the timeout limit and was killed."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-004", .internal_server_error, "Execution OOM kill", "Execution exceeded memory limit and was killed."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-005", .internal_server_error, "Execution resource kill", "Execution exceeded resource limits and was killed."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-006", .internal_server_error, "Execution transport loss", "Connection to execution transport was lost."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-007", .internal_server_error, "Execution lease expired", "Execution lease expired. The task took too long to complete."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-008", .internal_server_error, "Execution renewal-terminated", "The control plane stopped the lease mid-run (lease lost, max-runtime cap, or no credits). A policy stop, kept distinct from a wall-clock timeout (UZ-EXEC-003) for triage and billing."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-009", .internal_server_error, "Execution startup posture failure", "Execution startup posture check failed. Verify runner security config."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-010", .internal_server_error, "Execution crash", "The execution process crashed. Check logs for details."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-011", .forbidden, "Landlock policy deny", "Landlock policy denied the filesystem operation."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-012", .internal_server_error, "Runner fleet init failed", "Runner fleet initialization failed. Check configuration."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-013", .internal_server_error, "Runner fleet run failed", "Runner fleet execution failed. Check logs for details."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-014", .bad_request, "Runner invalid config", "Runner configuration is invalid. Check config_json fields."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-015", .payment_required, "Execution stopped: fleet budget exhausted", "The control plane refused the lease renewal because the fleet reached its own daily_dollars or monthly_dollars ceiling (UZ-RUN-015). A fleet-scoped spend stop, kept distinct from a renewal-terminate (UZ-EXEC-008) so triage can tell the fleet author's own limit from a platform or billing stop."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-016", .unauthorized, "Runner token rejected", "The control plane rejected this host's agt_r runner token (401/403) on the heartbeat. Retrying can't fix it — mint a fresh agt_r and re-provision the host's runner token, then restart the runner."), // reachable: no — runner control-loop code, logged (server_stopped reason=token_rejected) not dashboard-fetched
    // ── RELAY ────────────────────────────────────────────────────────────────
    // ── APPROVAL GATE ────────────────────────────────────────────────────────
    eu("UZ-APPROVAL-001", .bad_request, "Approval parse failed", "Gate policy in TRIGGER.md config_json has invalid syntax. Check the 'gates' section.", "That approval gate's config is invalid. Check the gates section in TRIGGER.md."),
    eu("UZ-APPROVAL-002", .not_found, "Approval not found", "Approval action not found or already resolved. " ++
        "The action may have timed out or been handled by another click.", "That approval action wasn't found. It may have already timed out or been resolved elsewhere."),
    eu("UZ-APPROVAL-003", .unauthorized, "Approval invalid signature", "The approval callback signature is invalid. Check the signing secret.", "That approval callback couldn't be verified. Check the signing secret configuration."),
    eu("UZ-APPROVAL-004", .service_unavailable, "Approval Redis unavailable", "Gate service unavailable \u{2014} default-deny applied. Check Redis connectivity.", "Approvals are temporarily unavailable. We default to denying while this is down — try again shortly."),
    eu("UZ-APPROVAL-005", .bad_request, "Approval condition invalid", "Gate condition expression is invalid. Supported operators: == and != with single-quoted values.", "That approval gate's condition is invalid. Check the gate's condition expression for a supported operator."),
    eu("UZ-APPROVAL-006", .conflict, "Approval already resolved", "Resolved earlier by Slack, dashboard, or auto-timeout. Original outcome + resolver in body.", S_ALREADY_RESOLVED_USER_MSG),
    // ── MEMORY ───────────────────────────────────────────────────────────────
    e("UZ-MEM-002", .not_found, "Fleet not found for memory op", "The fleet_id does not exist or does not belong to the requesting workspace. " ++
        "Verify the fleet_id and workspace scope."), // reachable: no — runner memory-push endpoint (fleet-side), not fetched by ui/packages/app
    e("UZ-MEM-003", .service_unavailable, "Memory backend unavailable", "The memory backend (Postgres memory schema) is unreachable. " ++
        "The fleet falls back to ephemeral workspace memory. Check MEMORY_RUNTIME_URL."), // reachable: no — runner memory-push endpoint (fleet-side), not fetched by ui/packages/app
    // ── AGENT KEYS (workspace-scoped, agt_a prefix) ────────────────────────────
    e("UZ-APIKEY-001", .unauthorized, "Invalid API key", "API key is invalid or revoked. Mint a replacement with: `POST /v1/workspaces/{ws}/fleet-keys`"), // reachable: no — fleet-scoped agt_a bearer auth (CLI/runner), not a browser session
    // ── TENANT API KEYS (tenant-scoped, agt_t prefix) ────────────────────────
    eu("UZ-APIKEY-003", .not_found, "API key not found", "No API key matches the supplied id for this tenant. Verify the id with: GET /v1/api-keys", "We couldn't find that API key. It may have already been deleted — refresh the list."),
    e("UZ-APIKEY-004", .unauthorized, "API key has been revoked", "This key was revoked and can no longer authenticate. Mint a replacement with: POST /v1/api-keys"), // reachable: no — CLI/API-key bearer-auth surface, not a browser session
    eu("UZ-APIKEY-005", .conflict, "Key name already exists in this tenant", "key_name must be unique per tenant. Pick a different name or revoke the existing key first.", "An API key with that name already exists. Pick a different name for this tenant."),
    eu("UZ-APIKEY-006", .conflict, "API key is already revoked", "This key is already revoked. No further action is required.", "That API key is already revoked. Refresh the list to see its current state."),
    eu("UZ-APIKEY-007", .conflict, "active cannot be set to true; mint a new key instead", "Re-activation is not supported. Create a new key via POST /v1/api-keys and revoke the old one.", "A revoked key can't be reactivated. Mint a new key instead."),
    eu("UZ-APIKEY-008", .conflict, "Active API key must be revoked before deletion", "Revoke the key first with `PATCH /v1/api-keys/{id}` body `{\"active\": false}`, then retry DELETE.", "Revoke this key before deleting it. Revoke it first, then delete the revoked key."),
    // ── INTEGRATION GRANTS ────────────────────────────────────────────────────
    // UZ-GRANT-001 restored (Jul 06, 2026): believed dead when M116 authored
    // its Dead Code Sweep — that grep matched the code STRING "UZ-GRANT-001",
    // which is correct for finding e()/eu() registry entries but blind to a
    // caller that references the derived ERR_* constant by name instead
    // (fleet/service.zig's grant-gate lease check, credentials_mint.zig's
    // on-demand mint gate — both landed in the grant-gated mint/lease PR,
    // merged concurrently with this branch). Restored verbatim; see this
    // spec's Discovery for the cross-PR collision this exposed.
    e("UZ-GRANT-001", .forbidden, "No integration grant for service", "This fleet has no approved grant for the target service. " ++
        "Request one with: `POST /v1/workspaces/{ws}/fleets/{id}/integration-requests`"), // reachable: no — runner-only mint/lease gate, not fetched by ui/packages/app
    eu("UZ-GRANT-002", .not_found, "Integration grant not found", "No grant with that id exists for this fleet, or it was already revoked. " ++
        "List current grants with: `GET /v1/workspaces/{ws}/fleets/{id}/integration-grants`", "We couldn't find that grant request. It may have already been resolved — refresh the list."),
    eu("UZ-GRANT-003", .conflict, "Grant already resolved", "This grant was already approved or denied \u{2014} by an earlier click, the dashboard, or an auto-timeout. " ++
        "The original decision stands; this request changed nothing.", S_ALREADY_RESOLVED_USER_MSG),
    // ── CREDENTIAL BROKER (M102 — on-demand mint) ─────────────────────────────
    // Surfaced first at POST /v1/runners/me/credentials/mint (the mint endpoint
    // is the first caller — registering them earlier would be caller-less, NDC).
    // No secret ever appears in these messages (VLT) — host/status only.
    eu("UZ-CRED-001", .not_found, "Integration not connected", "No connected integration matches this id for the fleet's workspace. " ++
        "Connect it first (e.g. GitHub via the dashboard \u{201c}Connect\u{201d} flow) before a fleet can mint a token for it.", "That integration isn't connected. Connect it from the Integrations page, then try again."),
    e("UZ-CRED-002", .service_unavailable, "Credential broker not configured", "The on-demand credential broker is not wired on this deployment (a boot-time misconfiguration, not a client error). An operator must configure it before runners can mint credentials."), // reachable: no — runner-only mint endpoint, not fetched by ui/packages/app
    e("UZ-GH-001", .conflict, "GitHub App reconnect required", "The GitHub App installation is gone (uninstalled or revoked), so no token can be minted. " ++
        "Reconnect GitHub from the dashboard \u{2014} the fleet stays blocked until the App is reinstalled."), // reachable: no — response goes to the runner's credential-mint call, surfaced to the agent as a tool failure, not to a dashboard fetch
    e("UZ-GH-002", .bad_gateway, "GitHub token mint failed", "GitHub did not return an installation token (upstream 5xx, network, or a malformed exchange response). " ++
        "This is transient \u{2014} retry shortly; if it persists, check GitHub status and the App configuration."), // reachable: no — response goes to the runner's credential-mint call, not to a dashboard fetch
    // ── CONNECTOR PLATFORM (the connect round-trip + bounded vendor calls) ────
    eu("UZ-CONN-001", .service_unavailable, "Connector not configured", "This connector's platform app is not provisioned on this deployment (its admin-vault secret bag, App slug, or signing secret is unset). " ++
        "An operator must register the provider app and populate the admin vault before workspaces can connect.", "This connector isn't set up on this deployment yet. Contact your operator to enable it."),
    eu("UZ-CONN-002", .bad_request, "Invalid connect state", "The connect callback's state was missing, forged, expired, or already used. " ++
        "Start the connect again from the dashboard \u{2014} each attempt issues a fresh single-use state.", "That connection attempt expired or was already used. Start connecting again from the dashboard."),
    eu("UZ-CONN-003", .bad_gateway, "Connector vendor call exceeded its deadline", "An outbound call to the connector's vendor hit its enforced deadline (the vendor accepted the connection, then stalled), could not be deadline-armed and was refused (watchdog unavailable), or the vendor was unreachable (dial/transport failure) \u{2014} the call never runs unbounded. " ++
        "Transient \u{2014} retry; if it persists, check the vendor's status page and this deployment's egress.", "We couldn't reach that service right now. Try again shortly."),
    eu("UZ-CONN-004", .not_found, "Unknown connector provider", "The `{provider}` segment does not match any provider in this deployment's connector registry. " ++
        "List the available providers from the dashboard connectors page (or the catalog endpoint once it ships).", "We don't recognize that connector. Check the available connectors on the dashboard."),
    eu("UZ-CONN-006", .bad_gateway, "Connector OAuth exchange failed", "The connector's OAuth code exchange or provider callback body was rejected. " ++
        "Start the connect again from the dashboard; if it repeats, verify the provider app credentials and redirect URL.", "That connection didn't go through. Try connecting again from the dashboard."),
    eu("UZ-CONN-007", .internal_server_error, "Connector catalog lookup failed", "The vault existence check for connector app/fleet keys failed (a Postgres error). " ++
        "Retry; if it persists, check DB connectivity and the vault schema state.", "We couldn't load your connectors right now. Try refreshing — if it keeps failing, contact support."),
    eu("UZ-CONN-008", .forbidden, "Connector installation ownership not verified", "GitHub did not confirm that the authorizing user can access the submitted App installation, or that installation is already connected to another workspace. " ++
        "Start the connection again while signed in to the GitHub account that owns the installation.", "We couldn't verify that this GitHub installation belongs to you. Sign in with the owning GitHub account and try again."),
};
