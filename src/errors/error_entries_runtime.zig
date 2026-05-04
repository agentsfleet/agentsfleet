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
    e("UZ-SANDBOX-001", .service_unavailable, "Sandbox backend unavailable",
        "Sandbox backend is not available. Check that bubblewrap (bwrap) is installed and accessible."),
    e("UZ-SANDBOX-002", .forbidden, "Sandbox kill switch triggered",
        "Sandbox kill switch has been triggered. Contact the administrator."),
    e("UZ-SANDBOX-003", .forbidden, "Sandbox command blocked",
        "The command is blocked by sandbox policy."),
    // ── EXECUTOR ─────────────────────────────────────────────────────────────
    e("UZ-EXEC-001", .internal_server_error, "Execution session create failed",
        "Execution session creation failed. Check runner availability."),
    e("UZ-EXEC-002", .internal_server_error, "Stage start failed",
        "Stage failed to start. Check runner configuration."),
    e("UZ-EXEC-003", .internal_server_error, "Execution timeout kill",
        "Execution exceeded the timeout limit and was killed."),
    e("UZ-EXEC-004", .internal_server_error, "Execution OOM kill",
        "Execution exceeded memory limit and was killed."),
    e("UZ-EXEC-005", .internal_server_error, "Execution resource kill",
        "Execution exceeded resource limits and was killed."),
    e("UZ-EXEC-006", .internal_server_error, "Execution transport loss",
        "Connection to execution transport was lost."),
    e("UZ-EXEC-007", .internal_server_error, "Execution lease expired",
        "Execution lease expired. The task took too long to complete."),
    e("UZ-EXEC-008", .forbidden, "Execution policy deny",
        "Execution was denied by policy. Check firewall rules."),
    e("UZ-EXEC-009", .internal_server_error, "Execution startup posture failure",
        "Execution startup posture check failed. Verify runner security config."),
    e("UZ-EXEC-010", .internal_server_error, "Execution crash",
        "The execution process crashed. Check logs for details."),
    e("UZ-EXEC-011", .forbidden, "Landlock policy deny",
        "Landlock policy denied the filesystem operation."),
    e("UZ-EXEC-012", .internal_server_error, "Runner agent init failed",
        "Runner agent initialization failed. Check configuration."),
    e("UZ-EXEC-013", .internal_server_error, "Runner agent run failed",
        "Runner agent execution failed. Check logs for details."),
    e("UZ-EXEC-014", .bad_request, "Runner invalid config",
        "Runner configuration is invalid. Check config_json fields."),
    // ── RELAY ────────────────────────────────────────────────────────────────
    e("UZ-RELAY-001", .bad_request, "No LLM provider configured",
        "No LLM provider configured. Set one via admin platform keys or workspace credentials."),
    // ── CREDENTIALS ──────────────────────────────────────────────────────────
    e("UZ-CRED-001", .service_unavailable, "Anthropic API key missing",
        "Workspace LLM API key not found in vault.secrets (key: anthropic_api_key). " ++
        "Set it via the workspace credentials API. " ++
        "Executor fell back to process env \u{2014} check ANTHROPIC_API_KEY on the worker if dev mode."),
    e("UZ-CRED-003", .service_unavailable, "Platform LLM key missing",
        "No active platform LLM key for this provider. Admin must set one via " ++
        "PUT /v1/admin/platform-keys, or the workspace must add its own key " ++
        "via PUT /v1/workspaces/{id}/credentials/llm."),
    // ── APPROVAL GATE ────────────────────────────────────────────────────────
    e("UZ-APPROVAL-001", .bad_request, "Approval parse failed",
        "Gate policy in TRIGGER.md config_json has invalid syntax. Check the 'gates' section."),
    e("UZ-APPROVAL-002", .not_found, "Approval not found",
        "Approval action not found or already resolved. " ++
        "The action may have timed out or been handled by another click."),
    e("UZ-APPROVAL-003", .unauthorized, "Approval invalid signature",
        "The approval callback signature is invalid. Check the signing secret."),
    e("UZ-APPROVAL-004", .service_unavailable, "Approval Redis unavailable",
        "Gate service unavailable \u{2014} default-deny applied. Check Redis connectivity."),
    e("UZ-APPROVAL-005", .bad_request, "Approval condition invalid", "Gate condition expression is invalid. Supported operators: == and != with single-quoted values."),
    e("UZ-APPROVAL-006", .conflict, "Approval already resolved", "Resolved earlier by Slack, dashboard, or auto-timeout. Original outcome + resolver in body."),
    // ── MEMORY ───────────────────────────────────────────────────────────────
    e("UZ-MEM-001", .forbidden, "Memory scope denied",
        "Memory request targets a zombie that belongs to a different workspace. " ++
        "Each zombie's memory is scoped to its own instance_id; cross-zombie access is not permitted."),
    e("UZ-MEM-002", .not_found, "Zombie not found for memory op",
        "The zombie_id does not exist or does not belong to the requesting workspace. " ++
        "Verify the zombie_id and workspace scope."),
    e("UZ-MEM-003", .service_unavailable, "Memory backend unavailable",
        "The memory backend (Postgres memory schema) is unreachable. " ++
        "The agent falls back to ephemeral workspace memory. Check MEMORY_RUNTIME_URL."),
    // ── AGENT KEYS (workspace-scoped, zmb_ prefix) ────────────────────────────
    e("UZ-APIKEY-001", .unauthorized, "Invalid API key",
        "API key is invalid or revoked. Create one with: zombiectl agent create --workspace {ws} --name my-agent"),
    e("UZ-APIKEY-002", .forbidden, "API key lacks execute permission",
        "This API key does not have execute permission. Re-create with the correct permissions."),
    // ── TENANT API KEYS (tenant-scoped, zmb_t_ prefix) ────────────────────────
    e("UZ-APIKEY-003", .not_found, "API key not found",
        "No API key matches the supplied id for this tenant. Verify the id with: GET /v1/api-keys"),
    e("UZ-APIKEY-004", .unauthorized, "API key has been revoked",
        "This key was revoked and can no longer authenticate. Mint a replacement with: POST /v1/api-keys"),
    e("UZ-APIKEY-005", .conflict, "Key name already exists in this tenant",
        "key_name must be unique per tenant. Pick a different name or revoke the existing key first."),
    e("UZ-APIKEY-006", .conflict, "API key is already revoked",
        "This key is already revoked. No further action is required."),
    e("UZ-APIKEY-007", .conflict, "active cannot be set to true; mint a new key instead",
        "Re-activation is not supported. Create a new key via POST /v1/api-keys and revoke the old one."),
    e("UZ-APIKEY-008", .conflict, "Active API key must be revoked before deletion",
        "Revoke the key first with PATCH /v1/api-keys/{id} body {\"active\": false}, then retry DELETE."),
    // ── INTEGRATION GRANTS ────────────────────────────────────────────────────
    e("UZ-GRANT-001", .forbidden, "No integration grant for service",
        "This zombie has no approved grant for the target service. " ++
        "Request one with: POST /v1/zombies/{id}/integration-requests"),
    e("UZ-GRANT-002", .forbidden, "Integration grant pending approval",
        "A grant request for this service is pending human approval. " ++
        "Approve it in Slack, Discord, or the dashboard."),
    e("UZ-GRANT-003", .forbidden, "Integration grant denied",
        "The integration grant for this service was denied or revoked by the workspace owner."),
    // ── FIREWALL (execute path) ───────────────────────────────────────────────
    e("UZ-FW-001", .forbidden, "Domain not in workspace allowlist",
        "The target domain is not in the workspace allowlist. Add it via firewall configuration."),
    e("UZ-FW-002", .bad_request, "Human approval required",
        "The request body triggered the approval gate. Awaiting human approval before execution."),
    e("UZ-FW-003", .forbidden, "Prompt injection detected",
        "A prompt injection pattern was detected in the request body. Request blocked."),
};
