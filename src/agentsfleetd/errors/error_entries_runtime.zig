/// error_entries_runtime.zig — runtime / execute-path error entries.
///
/// Sibling of error_entries.zig (control-plane entries). Split for the
/// 350-line file cap. Both arrays are concatenated by error_registry.zig.
const std = @import("std");
const entries = @import("error_entries.zig");
const Entry = entries.Entry;
const ERROR_DOCS_BASE = entries.ERROR_DOCS_BASE;

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

pub const ENTRIES_RUNTIME = [_]Entry{
    // ── SANDBOX ──────────────────────────────────────────────────────────────
    // ── RUNNER ─────────────────────────────────────────────────────────────
    e("UZ-EXEC-001", .internal_server_error, "Execution session create failed", "Execution session creation failed. Check runner availability."),
    e("UZ-EXEC-002", .internal_server_error, "Run start failed", "Run failed to start. Check runner configuration."),
    e("UZ-EXEC-003", .internal_server_error, "Execution timeout kill", "Execution exceeded the timeout limit and was killed."),
    e("UZ-EXEC-004", .internal_server_error, "Execution OOM kill", "Execution exceeded memory limit and was killed."),
    e("UZ-EXEC-005", .internal_server_error, "Execution resource kill", "Execution exceeded resource limits and was killed."),
    e("UZ-EXEC-006", .internal_server_error, "Execution transport loss", "Connection to execution transport was lost."),
    e("UZ-EXEC-007", .internal_server_error, "Execution lease expired", "Execution lease expired. The task took too long to complete."),
    e("UZ-EXEC-008", .internal_server_error, "Execution renewal-terminated", "The control plane stopped the lease mid-run (lease lost, max-runtime cap, or no credits). A policy stop, kept distinct from a wall-clock timeout (UZ-EXEC-003) for triage and billing."),
    e("UZ-EXEC-009", .internal_server_error, "Execution startup posture failure", "Execution startup posture check failed. Verify runner security config."),
    e("UZ-EXEC-010", .internal_server_error, "Execution crash", "The execution process crashed. Check logs for details."),
    e("UZ-EXEC-011", .forbidden, "Landlock policy deny", "Landlock policy denied the filesystem operation."),
    e("UZ-EXEC-012", .internal_server_error, "Runner fleet init failed", "Runner fleet initialization failed. Check configuration."),
    e("UZ-EXEC-013", .internal_server_error, "Runner fleet run failed", "Runner fleet execution failed. Check logs for details."),
    e("UZ-EXEC-014", .bad_request, "Runner invalid config", "Runner configuration is invalid. Check config_json fields."),
    // ── RELAY ────────────────────────────────────────────────────────────────
    // ── APPROVAL GATE ────────────────────────────────────────────────────────
    e("UZ-APPROVAL-001", .bad_request, "Approval parse failed", "Gate policy in TRIGGER.md config_json has invalid syntax. Check the 'gates' section."),
    e("UZ-APPROVAL-002", .not_found, "Approval not found", "Approval action not found or already resolved. " ++
        "The action may have timed out or been handled by another click."),
    e("UZ-APPROVAL-003", .unauthorized, "Approval invalid signature", "The approval callback signature is invalid. Check the signing secret."),
    e("UZ-APPROVAL-004", .service_unavailable, "Approval Redis unavailable", "Gate service unavailable \u{2014} default-deny applied. Check Redis connectivity."),
    e("UZ-APPROVAL-005", .bad_request, "Approval condition invalid", "Gate condition expression is invalid. Supported operators: == and != with single-quoted values."),
    e("UZ-APPROVAL-006", .conflict, "Approval already resolved", "Resolved earlier by Slack, dashboard, or auto-timeout. Original outcome + resolver in body."),
    // ── MEMORY ───────────────────────────────────────────────────────────────
    e("UZ-MEM-002", .not_found, "Fleet not found for memory op", "The fleet_id does not exist or does not belong to the requesting workspace. " ++
        "Verify the fleet_id and workspace scope."),
    e("UZ-MEM-003", .service_unavailable, "Memory backend unavailable", "The memory backend (Postgres memory schema) is unreachable. " ++
        "The fleet falls back to ephemeral workspace memory. Check MEMORY_RUNTIME_URL."),
    // ── AGENT KEYS (workspace-scoped, agt_a prefix) ────────────────────────────
    e("UZ-APIKEY-001", .unauthorized, "Invalid API key", "API key is invalid or revoked. Create one with: agentsfleet fleet create --workspace {ws} --name my-fleet"),
    // ── TENANT API KEYS (tenant-scoped, agt_t prefix) ────────────────────────
    e("UZ-APIKEY-003", .not_found, "API key not found", "No API key matches the supplied id for this tenant. Verify the id with: GET /v1/api-keys"),
    e("UZ-APIKEY-004", .unauthorized, "API key has been revoked", "This key was revoked and can no longer authenticate. Mint a replacement with: POST /v1/api-keys"),
    e("UZ-APIKEY-005", .conflict, "Key name already exists in this tenant", "key_name must be unique per tenant. Pick a different name or revoke the existing key first."),
    e("UZ-APIKEY-006", .conflict, "API key is already revoked", "This key is already revoked. No further action is required."),
    e("UZ-APIKEY-007", .conflict, "active cannot be set to true; mint a new key instead", "Re-activation is not supported. Create a new key via POST /v1/api-keys and revoke the old one."),
    e("UZ-APIKEY-008", .conflict, "Active API key must be revoked before deletion", "Revoke the key first with PATCH /v1/api-keys/{id} body {\"active\": false}, then retry DELETE."),
    // ── INTEGRATION GRANTS ────────────────────────────────────────────────────
    e("UZ-GRANT-001", .forbidden, "No integration grant for service", "This fleet has no approved grant for the target service. " ++
        "Request one with: POST /v1/fleets/{id}/integration-requests"),
    e("UZ-GRANT-002", .not_found, "Integration grant not found", "No grant with that id exists for this fleet, or it was already revoked. " ++
        "List current grants with: GET /v1/workspaces/{ws}/fleets/{id}/integration-grants"),
    e("UZ-GRANT-003", .conflict, "Grant already resolved", "This grant was already approved or denied \u{2014} by an earlier click, the dashboard, or an auto-timeout. " ++
        "The original decision stands; this request changed nothing."),
    // ── CREDENTIAL BROKER (M102 — on-demand mint) ─────────────────────────────
    // Surfaced first at POST /v1/runners/me/credentials/mint (the mint endpoint
    // is the first caller — registering them earlier would be caller-less, NDC).
    // No secret ever appears in these messages (VLT) — host/status only.
    e("UZ-CRED-001", .not_found, "Integration not connected", "No connected integration matches this id for the fleet's workspace. " ++
        "Connect it first (e.g. GitHub via the dashboard \u{201c}Connect\u{201d} flow) before a fleet can mint a token for it."),
    e("UZ-GH-001", .conflict, "GitHub App reconnect required", "The GitHub App installation is gone (uninstalled or revoked), so no token can be minted. " ++
        "Reconnect GitHub from the dashboard \u{2014} the fleet stays blocked until the App is reinstalled."),
    e("UZ-GH-002", .bad_gateway, "GitHub token mint failed", "GitHub did not return an installation token (upstream 5xx, network, or a malformed exchange response). " ++
        "This is transient \u{2014} retry shortly; if it persists, check GitHub status and the App configuration."),
    // ── CONNECTOR PLATFORM (the connect round-trip + bounded vendor calls) ────
    e("UZ-CONN-001", .service_unavailable, "Connector not configured", "This connector's platform app is not provisioned on this deployment (its admin-vault secret bag, App slug, or signing secret is unset). " ++
        "An operator must register the provider app and populate the admin vault before workspaces can connect."),
    e("UZ-CONN-002", .bad_request, "Invalid connect state", "The connect callback's state was missing, forged, expired, or already used. " ++
        "Start the connect again from the dashboard \u{2014} each attempt issues a fresh single-use state."),
    e("UZ-CONN-003", .bad_gateway, "Connector vendor call exceeded its deadline", "An outbound call to the connector's vendor hit its enforced deadline (the vendor accepted the connection, then stalled), could not be deadline-armed and was refused (watchdog unavailable), or the vendor was unreachable (dial/transport failure) \u{2014} the call never runs unbounded. " ++
        "Transient \u{2014} retry; if it persists, check the vendor's status page and this deployment's egress."),
    e("UZ-CONN-004", .not_found, "Unknown connector provider", "The {provider} segment does not match any provider in this deployment's connector registry. " ++
        "List the available providers from the dashboard connectors page (or the catalog endpoint once it ships)."),
    e("UZ-CONN-006", .bad_gateway, "Connector OAuth exchange failed", "The connector's OAuth code exchange or provider callback body was rejected. " ++
        "Start the connect again from the dashboard; if it repeats, verify the provider app credentials and redirect URL."),
};
