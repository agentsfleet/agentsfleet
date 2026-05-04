/// error_entries.zig — single source of truth for all error code entries.
///
/// Each Entry has 5 required fields: code, http_status, title, hint, docs_uri.
/// The ENTRIES array is imported by error_registry.zig which builds the
/// comptime lookup map and re-exports ERR_* constants.
///
/// To add a new error code: add one e() call to ENTRIES, then add the
/// corresponding ERR_* constant in error_registry.zig.
const std = @import("std");

pub const ERROR_DOCS_BASE = "https://docs.usezombie.com/error-codes#";

pub const Entry = struct {
    code: []const u8,
    http_status: std.http.Status,
    title: []const u8,
    hint: []const u8,
    docs_uri: []const u8,
};

/// Sentinel for unrecognized codes. Defined OUTSIDE ENTRIES — collision
/// is structurally impossible (enforced by comptime assertion in error_registry.zig).
pub const UNKNOWN = Entry{
    .code = "UZ-UNKNOWN",
    .http_status = .internal_server_error,
    .title = "Unknown error",
    .hint = "This error code is not registered. Report to the operator.",
    .docs_uri = ERROR_DOCS_BASE ++ "UZ-INTERNAL-003",
};

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

pub const ENTRIES = [_]Entry{
    // ── UUIDV7 ──────────────────────────────────────────────────────────────
    e("UZ-UUIDV7-003", .bad_request, "Invalid UUID canonical format",
        "ID must be a valid UUIDv7 in canonical 8-4-4-4-12 hex format."),
    e("UZ-UUIDV7-005", .internal_server_error, "ID generation failed",
        "Internal ID generation failed. Retry the request."),
    e("UZ-UUIDV7-009", .bad_request, "Invalid ID shape",
        "The supplied ID does not match the expected UUIDv7 shape."),
    e("UZ-UUIDV7-010", .conflict, "UUID backfill conflict",
        "UUID backfill found a conflicting existing ID. Resolve manually."),
    e("UZ-UUIDV7-011", .internal_server_error, "Rollback blocked",
        "Rollback is blocked by a dependent resource. Check constraints."),
    e("UZ-UUIDV7-012", .internal_server_error, "Error response linking failed",
        "Error response could not be linked to the originating request."),
    // ── INTERNAL ─────────────────────────────────────────────────────────────
    e("UZ-INTERNAL-001", .service_unavailable, "Database unavailable",
        "Check that DATABASE_URL is set and the database server is reachable. Run 'zombied doctor' to verify."),
    e("UZ-INTERNAL-002", .internal_server_error, "Database error",
        "A database query failed. Check the err= field and database logs."),
    e("UZ-INTERNAL-003", .internal_server_error, "Internal operation failed",
        "An internal operation failed. Check the err= field for details. " ++
        "If persistent, check service connectivity and run 'zombied doctor'."),
    // ── REQUEST ──────────────────────────────────────────────────────────────
    e("UZ-REQ-001", .bad_request, "Invalid request",
        "The request body or parameters are invalid. Check the API documentation."),
    e("UZ-REQ-002", .payload_too_large, "Payload too large",
        "Request body exceeds the maximum allowed size."),
    // ── AUTH ─────────────────────────────────────────────────────────────────
    e("UZ-AUTH-001", .forbidden, "Forbidden",
        "Access denied. Check that your API key has the required role."),
    e("UZ-AUTH-002", .unauthorized, "Unauthorized",
        "Authentication required. Provide a valid Bearer token."),
    e("UZ-AUTH-003", .unauthorized, "Token expired",
        "Your authentication token has expired. Re-authenticate."),
    e("UZ-AUTH-004", .service_unavailable, "Authentication service unavailable",
        "Authentication service is temporarily unavailable. Retry shortly."),
    e("UZ-AUTH-005", .not_found, "Session not found",
        "Session was not found. It may have expired or been invalidated."),
    e("UZ-AUTH-006", .unauthorized, "Session expired",
        "Your session has expired. Please sign in again."),
    e("UZ-AUTH-007", .conflict, "Session already complete",
        "This session has already been completed and cannot be reused."),
    e("UZ-AUTH-008", .service_unavailable, "Session limit reached",
        "Maximum concurrent sessions reached. Close an existing session first."),
    e("UZ-AUTH-009", .forbidden, "Insufficient role",
        "Your role does not have sufficient permissions for this action."),
    e("UZ-AUTH-010", .forbidden, "Unsupported role",
        "The specified role is not supported."),
    // ── API / QUEUE ──────────────────────────────────────────────────────────
    e("UZ-API-001", .service_unavailable, "API saturated",
        "API is at capacity. Retry with exponential backoff."),
    e("UZ-API-002", .service_unavailable, "Queue unavailable",
        "Event queue is unavailable. Check Redis connectivity."),
    // ── WORKSPACE ────────────────────────────────────────────────────────────
    e("UZ-WORKSPACE-001", .not_found, "Workspace not found",
        "Workspace not found. Verify the workspace ID."),
    e("UZ-WORKSPACE-002", .payment_required, "Workspace paused",
        "Workspace is paused due to billing. Update payment method."),
    e("UZ-WORKSPACE-003", .payment_required, "Workspace free limit reached",
        "Free tier workspace limit reached. Upgrade your plan."),
    // ── BILLING ──────────────────────────────────────────────────────────────
    e("UZ-BILLING-001", .bad_request, "Invalid subscription ID",
        "The subscription ID is invalid or malformed."),
    e("UZ-BILLING-002", .internal_server_error, "Billing state missing",
        "Billing state record is missing. Contact support."),
    e("UZ-BILLING-003", .internal_server_error, "Billing state invalid",
        "Billing state is in an invalid state. Contact support."),
    e("UZ-BILLING-004", .bad_request, "Invalid billing event",
        "The billing event payload is invalid."),
    e("UZ-BILLING-005", .payment_required, "Credit exhausted",
        "Workspace credits are exhausted. Add credits or upgrade plan."),
    // ── SCORING ──────────────────────────────────────────────────────────────
    e("UZ-SCORING-001", .bad_request, "Invalid scoring context",
        "Scoring context tokens are invalid. Check the context_tokens field."),
    // ── ENTITLEMENT ──────────────────────────────────────────────────────────
    e("UZ-ENTL-001", .service_unavailable, "Entitlement service unavailable",
        "Entitlement service is temporarily unavailable. Retry shortly."),
    e("UZ-ENTL-003", .payment_required, "Stage limit reached",
        "Stage limit reached for your plan. Upgrade to add more."),
    e("UZ-ENTL-004", .forbidden, "Skill not allowed",
        "This skill is not allowed on your current plan."),
    // ── AGENT ────────────────────────────────────────────────────────────────
    e("UZ-AGENT-001", .not_found, "Agent not found",
        "Agent not found. Verify the agent_id."),
    // ── PROFILE ──────────────────────────────────────────────────────────────
    e("UZ-PROFILE-001", .not_found, "Profile not found",
        "Profile not found. Verify the profile_id."),
    e("UZ-PROFILE-002", .bad_request, "Invalid profile",
        "Profile data is invalid. Check required fields."),
    // ── WEBHOOK ──────────────────────────────────────────────────────────────
    e("UZ-WH-001", .not_found, "Zombie not found for webhook",
        "No zombie is registered for this webhook endpoint."),
    e("UZ-WH-002", .bad_request, "Malformed webhook",
        "Webhook payload could not be parsed. Check Content-Type and body."),
    e("UZ-WH-003", .conflict, "Zombie paused",
        "The target zombie is paused. Resume it before sending webhooks."),
    e("UZ-WH-010", .unauthorized, "Invalid webhook signature",
        "Webhook signature verification failed. Confirm the signing secret " ++
        "stored for this provider (Slack/Clerk/other) matches the one configured " ++
        "upstream."),
    e("UZ-WH-011", .unauthorized, "Stale webhook timestamp",
        "Webhook request timestamp is outside the allowed 5-minute drift window. " ++
        "This may indicate a replay attack or clock skew."),
    e("UZ-WH-020", .unauthorized, "Webhook credential not configured",
        "No webhook credential is configured for this zombie's source. Run " ++
        "`zombiectl credential add <source> --data='{\"webhook_secret\":\"...\"}'` " ++
        "in the zombie's workspace, then resend."),
    e("UZ-WH-030", .payload_too_large, "Webhook payload too large",
        "Webhook body exceeds the 1 MiB ingest limit. Reduce the payload size " ++
        "or filter at the source."),
    // ── TOOL ─────────────────────────────────────────────────────────────────
    e("UZ-TOOL-001", .failed_dependency, "Tool credential missing",
        "A required credential is not in the vault. Add it with: zombiectl credential add <skill_name>"),
    e("UZ-TOOL-002", .bad_gateway, "Tool API call failed",
        "Tool API call failed. Check the target service status and credential permissions."),
    e("UZ-TOOL-003", .bad_gateway, "Tool git operation failed",
        "Git operation failed. Check repo URL, branch name, and credential permissions."),
    e("UZ-TOOL-004", .bad_request, "Tool not attached",
        "The tool is not in this Zombie's tools list. Add it to the TRIGGER.md tools: section."),
    e("UZ-TOOL-005", .bad_request, "Unknown tool",
        "Unknown tool name. Check spelling against the known tools list."),
    e("UZ-TOOL-006", .gateway_timeout, "Tool call timed out",
        "Tool call timed out. Check network connectivity and target service status."),
    // ── ZOMBIE ───────────────────────────────────────────────────────────────
    e("UZ-ZMB-001", .payment_required, "Zombie budget exceeded",
        "Zombie hit its daily budget. Increase with: zombiectl config set budget.daily_dollars <amount>"),
    e("UZ-ZMB-002", .internal_server_error, "Zombie agent timeout",
        "Agent timed out processing an event. Check activity stream for details: zombiectl logs"),
    e("UZ-ZMB-003", .failed_dependency, "Zombie credential missing",
        "A required credential is not in the vault. Add it with: zombiectl credential add <name>"),
    e("UZ-ZMB-004", .internal_server_error, "Zombie claim failed",
        "Zombie could not be claimed from the database. Check that the zombie_id exists and status is 'active'."),
    e("UZ-ZMB-005", .internal_server_error, "Zombie checkpoint failed",
        "Session checkpoint write to Postgres failed. Check database connectivity."),
    e("UZ-ZMB-006", .conflict, "Zombie name already exists",
        "A Zombie with this name already exists. Use 'zombiectl kill <name>' first, then deploy again."),
    // UZ-ZMB-007 retired (single-string credential body) → see UZ-VAULT-002.
    e("UZ-ZMB-008", .bad_request, "Invalid zombie config",
        "Config JSON is malformed. Verify trigger, tools, credentials, and budget fields " ++
        "in your TRIGGER.md frontmatter. See samples/platform-ops/TRIGGER.md for a working example."),
    e("UZ-ZMB-009", .not_found, "Zombie not found",
        "Zombie not found. Verify the zombie_id and that it has not been killed."),
    e("UZ-ZMB-010", .conflict, "Zombie already stopped or killed",
        "This zombie is already stopped or has been killed. Restart it before issuing another stop."),
    e("UZ-ZMB-011", .bad_request, "SKILL.md and TRIGGER.md disagree on `name:`",
        "Top-level `name:` in SKILL.md must match `name:` in TRIGGER.md. One identity per zombie bundle."),
    // ── VAULT ────────────────────────────────────────────────────────────────
    e("UZ-VAULT-001", .bad_request, "Credential data must be a non-empty JSON object",
        "POST /credentials body must include a 'data' field that is a JSON object with at least one key. " ++
        "Bare strings, arrays, scalars, and {} are rejected."),
    e("UZ-VAULT-002", .bad_request, "Credential data too large",
        "Stringified credential data exceeds 4KB. Compose the secret from fewer or shorter fields."),
    // ── PROVIDER (PUT /v1/tenants/me/provider) ───────────────────────────────
    e("UZ-PROVIDER-001", .bad_request, "credential_ref required when mode=byok",
        "PUT body must include `credential_ref` naming a vault credential when `mode` is byok."),
    e("UZ-PROVIDER-002", .bad_request, "Credential row not found in vault",
        "The named credential_ref has no vault row in the tenant's primary workspace. " ++
        "Run `zombiectl credential set <name> --data @-` to create it."),
    e("UZ-PROVIDER-003", .bad_request, "Credential JSON missing required field",
        "Stored credential JSON must include `provider`, `api_key`, and `model` (all non-empty strings). " ++
        "Re-run `zombiectl credential set` with the full triplet."),
    e("UZ-PROVIDER-004", .bad_request, "Model not in cached caps catalogue",
        "The effective model is not present in core.model_caps. Pick a model from the model-caps endpoint " ++
        "or request the catalogue be extended."),
    // ── GATE ─────────────────────────────────────────────────────────────────
    e("UZ-GATE-001", .internal_server_error, "Gate command failed",
        "A gate command (make lint/test/build) failed. Check the gate results for stdout/stderr output."),
    e("UZ-GATE-002", .gateway_timeout, "Gate command timed out",
        "A gate command exceeded its timeout. Increase GATE_TOOL_TIMEOUT_MS or optimize the command."),
    e("UZ-GATE-003", .internal_server_error, "Gate repair attempts exhausted",
        "Agent exhausted all repair attempts without passing gates. " ++
        "Review gate results for the repeated failure pattern."),
    e("UZ-GATE-004", .internal_server_error, "Gate persist failed",
        "Gate results could not be written to the database. " ++
        "Check DB connectivity and that the gate_results table exists."),
    // ── STARTUP ──────────────────────────────────────────────────────────────
    e("UZ-STARTUP-001", .internal_server_error, "Environment check failed",
        "Required environment variables are missing. Run 'zombied doctor' to see which ones."),
    e("UZ-STARTUP-002", .internal_server_error, "Config load failed",
        "Configuration failed to load. Check that all required env vars are set. " ++
        "Run 'zombied doctor' to verify."),
    e("UZ-STARTUP-003", .internal_server_error, "Database connect failed",
        "Database is unreachable. Check that DATABASE_URL is set and the database accepts connections."),
    e("UZ-STARTUP-004", .internal_server_error, "Redis connect failed",
        "Redis is unreachable. Check that REDIS_URL_API and REDIS_URL_WORKER are set " ++
        "and the Redis server accepts connections. Run 'zombied doctor' to verify."),
    e("UZ-STARTUP-005", .internal_server_error, "Migration check failed",
        "Database migration state could not be verified. Check DB connectivity."),
    e("UZ-STARTUP-006", .internal_server_error, "OIDC init failed",
        "OIDC provider initialization failed. Check OIDC configuration."),
    e("UZ-STARTUP-007", .internal_server_error, "Redis group creation failed",
        "Redis connected but consumer group creation failed. " ++
        "Check Redis ACL permissions allow XGROUP CREATE."),
    // Runtime / execute-path entries (sandbox, executor, relay, credentials,
    // approval-gate, memory, api-keys, grants, tool/credential, proxy,
    // gate-execute) live in error_entries_runtime.zig and are concatenated
    // into REGISTRY by error_registry.zig.
};
