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

const S_UZ_INTERNAL_003 = "UZ-INTERNAL-003";

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
    .docs_uri = ERROR_DOCS_BASE ++ S_UZ_INTERNAL_003,
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
    e("UZ-UUIDV7-003", .bad_request, "Invalid UUID canonical format", "ID must be a valid UUIDv7 in canonical 8-4-4-4-12 hex format."),
    e("UZ-UUIDV7-005", .internal_server_error, "ID generation failed", "Internal ID generation failed. Retry the request."),
    e("UZ-UUIDV7-009", .bad_request, "Invalid ID shape", "The supplied ID does not match the expected UUIDv7 shape."),
    e("UZ-UUIDV7-010", .conflict, "UUID backfill conflict", "UUID backfill found a conflicting existing ID. Resolve manually."),
    e("UZ-UUIDV7-011", .internal_server_error, "Rollback blocked", "Rollback is blocked by a dependent resource. Check constraints."),
    e("UZ-UUIDV7-012", .internal_server_error, "Error response linking failed", "Error response could not be linked to the originating request."),
    // ── INTERNAL ─────────────────────────────────────────────────────────────
    e("UZ-INTERNAL-001", .service_unavailable, "Database unavailable", "Check that DATABASE_URL is set and the database server is reachable. Run 'zombied doctor' to verify."),
    e("UZ-INTERNAL-002", .internal_server_error, "Database error", "A database query failed. Check the err= field and database logs."),
    e(S_UZ_INTERNAL_003, .internal_server_error, "Internal operation failed", "An internal operation failed. Check the err= field for details. " ++
        "If persistent, check service connectivity and run 'zombied doctor'."),
    // ── REQUEST ──────────────────────────────────────────────────────────────
    e("UZ-REQ-001", .bad_request, "Invalid request", "The request body or parameters are invalid. Check the API documentation."),
    e("UZ-REQ-002", .payload_too_large, "Payload too large", "Request body exceeds the maximum allowed size."),
    // ── AUTH ─────────────────────────────────────────────────────────────────
    e("UZ-AUTH-001", .forbidden, "Forbidden", "Access denied. Check that your API key has the required role."),
    e("UZ-AUTH-002", .unauthorized, "Unauthorized", "Authentication required. Provide a valid Bearer token."),
    e("UZ-AUTH-003", .unauthorized, "Token expired", "Your authentication token has expired. Re-authenticate."),
    e("UZ-AUTH-004", .service_unavailable, "Authentication service unavailable", "Authentication service is temporarily unavailable. Retry shortly."),
    e("UZ-AUTH-005", .not_found, "Session not found", "Session was not found. It may have expired or been invalidated."),
    e("UZ-AUTH-006", .unauthorized, "Session expired", "Your session has expired. Please sign in again."),
    e("UZ-AUTH-007", .conflict, "Session already complete", "This session has already been completed and cannot be reused."),
    e("UZ-AUTH-008", .service_unavailable, "Session limit reached", "Maximum concurrent sessions reached. Close an existing session first."),
    e("UZ-AUTH-009", .forbidden, "Insufficient role", "Your role does not have sufficient permissions for this action."),
    e("UZ-AUTH-010", .forbidden, "Unsupported role", "The specified role is not supported."),
    e("UZ-AUTH-011", .bad_request, "Verification code did not match", "The 6-digit verification code did not match what the dashboard issued. " ++
        "Double-check the code shown in your browser and try again."),
    e("UZ-AUTH-012", .gone, "Login session already consumed", "This login session has already been consumed. Start over with `zombiectl login`."),
    e("UZ-AUTH-013", .gone, "Login session aborted", "This login session was aborted (too many wrong codes, explicit cancel, or replaced by a newer session). " ++
        "Start over with `zombiectl login`."),
    e("UZ-AUTH-014", .gone, "Login session not approved", "This login session has not been approved in the dashboard yet. " ++
        "Approve it in your browser before submitting a verification code."),
    e("UZ-AUTH-015", .conflict, "Login session already approved", "This login session has already been approved. Do not call /approve a second time."),
    e("UZ-AUTH-016", .bad_request, "Invalid CLI public key", "The supplied public_key is malformed. Expect base64url-encoded P-256 SubjectPublicKeyInfo."),
    e("UZ-AUTH-017", .bad_request, "Invalid token name", "token_name must be 1-64 characters of printable ASCII."),
    e("UZ-AUTH-018", .bad_request, "Invalid verification code shape", "verification_code must be exactly 6 ASCII digits."),
    e("UZ-AUTH-019", .bad_request, "Invalid ciphertext", "ciphertext is missing or empty. Expect a base64url-encoded AES-256-GCM output."),
    e("UZ-AUTH-020", .bad_request, "Invalid nonce", "nonce is missing, empty, or the wrong length. Expect a base64url-encoded 12-byte value."),
    e("UZ-AUTH-021", .forbidden, "Platform-admin privileges required", "This action is restricted to usezombie platform operators. Your account does not carry platform-admin privileges."),
    // ── WORKSPACE ────────────────────────────────────────────────────────────
    e("UZ-WORKSPACE-001", .not_found, "Workspace not found", "Workspace not found. Verify the workspace ID."),
    e("UZ-WORKSPACE-002", .payment_required, "Workspace paused", "Workspace is paused due to billing. Update payment method."),
    e("UZ-WORKSPACE-003", .payment_required, "Free tier limit reached", "Workspace has reached its free-tier limit. Add a payment method to continue."),
    // ── BILLING ──────────────────────────────────────────────────────────────
    e("UZ-BILLING-001", .service_unavailable, "Billing unavailable", "Billing service is temporarily unavailable. Retry shortly."),
    e("UZ-BILLING-002", .conflict, "Billing state missing", "No billing state recorded for this workspace. Contact support."),
    e("UZ-BILLING-003", .unprocessable_entity, "Billing state invalid", "Billing state is in an invalid shape. Contact support."),
    e("UZ-BILLING-004", .bad_request, "Invalid billing event", "Billing event payload could not be processed."),
    e("UZ-BILLING-005", .payment_required, "Credit exhausted", "Workspace credits are exhausted. Add credits or upgrade plan."),
    // ── AGENT ────────────────────────────────────────────────────────────────
    e("UZ-AGENT-001", .not_found, "Agent not found", "Agent not found. Verify the agent_id."),
    // ── WEBHOOK ──────────────────────────────────────────────────────────────
    e("UZ-WH-001", .not_found, "Zombie not found for webhook", "No zombie is registered for this webhook endpoint."),
    e("UZ-WH-002", .bad_request, "Malformed webhook", "Webhook payload could not be parsed. Check Content-Type and body."),
    e("UZ-WH-003", .conflict, "Zombie paused", "The target zombie is paused. Resume it before sending webhooks."),
    e("UZ-WH-010", .unauthorized, "Invalid webhook signature", "Webhook signature verification failed. Confirm the signing secret " ++
        "stored for this provider (Slack/Clerk/other) matches the one configured " ++
        "upstream."),
    e("UZ-WH-011", .unauthorized, "Stale webhook timestamp", "Webhook request timestamp is outside the allowed 5-minute drift window. " ++
        "This may indicate a replay attack or clock skew."),
    e("UZ-WH-020", .unauthorized, "Webhook credential not configured", "No webhook credential is configured for this zombie's source. Run " ++
        "`zombiectl credential add <source> --data='{\"webhook_secret\":\"...\"}'` " ++
        "in the zombie's workspace, then resend."),
    e("UZ-WH-030", .payload_too_large, "Webhook payload too large", "Webhook body exceeds the 1 MiB ingest limit. Reduce the payload size " ++
        "or filter at the source."),
    // ── TOOL ─────────────────────────────────────────────────────────────────
    e("UZ-TOOL-005", .bad_request, "Unknown tool", "Unknown tool name. Check spelling against the known tools list."),
    // ── ZOMBIE ───────────────────────────────────────────────────────────────
    e("UZ-ZMB-001", .payment_required, "Zombie budget exceeded", "Zombie hit its daily budget. Increase with: zombiectl config set budget.daily_dollars <amount>"),
    e("UZ-ZMB-002", .internal_server_error, "Zombie agent timeout", "Agent timed out processing an event. Check activity stream for details: zombiectl logs"),
    e("UZ-ZMB-003", .failed_dependency, "Zombie credential missing", "A required credential is not in the vault. Add it with: zombiectl credential add <name>"),
    e("UZ-ZMB-004", .internal_server_error, "Zombie claim failed", "Zombie could not be claimed from the database. Check that the zombie_id exists and status is 'active'."),
    e("UZ-ZMB-005", .internal_server_error, "Zombie checkpoint failed", "Session checkpoint write to Postgres failed. Check database connectivity."),
    e("UZ-ZMB-006", .conflict, "Zombie name already exists", "A Zombie with this name already exists. Use 'zombiectl kill <name>' first, then deploy again."),
    // UZ-ZMB-007 retired (single-string credential body) → see UZ-VAULT-002.
    e("UZ-ZMB-008", .bad_request, "Invalid zombie config", "Config JSON is malformed. Verify trigger, tools, credentials, and budget fields " ++
        "in your TRIGGER.md frontmatter. See samples/platform-ops/TRIGGER.md for a working example."),
    e("UZ-ZMB-009", .not_found, "Zombie not found", "Zombie not found. Verify the zombie_id and that it has not been killed."),
    e("UZ-ZMB-010", .conflict, "Zombie already stopped or killed", "This zombie is already stopped or has been killed. Restart it before issuing another stop."),
    e("UZ-ZMB-011", .bad_request, "SKILL.md and TRIGGER.md disagree on `name:`", "Top-level `name:` in SKILL.md must match `name:` in TRIGGER.md. One identity per zombie bundle."),
    // ── VAULT ────────────────────────────────────────────────────────────────
    e("UZ-VAULT-001", .bad_request, "Credential data must be a non-empty JSON object", "POST /credentials body must include a 'data' field that is a JSON object with at least one key. " ++
        "Bare strings, arrays, scalars, and {} are rejected."),
    e("UZ-VAULT-002", .bad_request, "Credential data too large", "Stringified credential data exceeds 4KB. Compose the secret from fewer or shorter fields."),
    // ── PROVIDER (PUT /v1/tenants/me/provider) ───────────────────────────────
    e("UZ-PROVIDER-001", .bad_request, "credential_ref required when mode=self_managed", "PUT body must include `credential_ref` naming a vault credential when `mode` is self_managed."),
    e("UZ-PROVIDER-002", .bad_request, "Credential row not found in vault", "The named credential_ref has no vault row in the tenant's primary workspace. " ++
        "Run `zombiectl credential set <name> --data @-` to create it."),
    e("UZ-PROVIDER-003", .bad_request, "Credential JSON missing required field", "Stored credential JSON must include `provider`, `api_key`, and `model` (all non-empty strings). " ++
        "Re-run `zombiectl credential set` with the full triplet."),
    e("UZ-PROVIDER-004", .bad_request, "Model not in cached caps catalogue", "The effective model is not present in core.model_caps. Pick a model from the model-caps endpoint " ++
        "or request the catalogue be extended."),
    // ── GATE ─────────────────────────────────────────────────────────────────
    e("UZ-GATE-001", .internal_server_error, "Gate command failed", "A gate command (make lint/test/build) failed. Check the gate results for stdout/stderr output."),
    e("UZ-GATE-002", .gateway_timeout, "Gate command timed out", "A gate command exceeded its timeout. Increase GATE_TOOL_TIMEOUT_MS or optimize the command."),
    e("UZ-GATE-003", .internal_server_error, "Gate repair attempts exhausted", "Agent exhausted all repair attempts without passing gates. " ++
        "Review gate results for the repeated failure pattern."),
    // ── STARTUP ──────────────────────────────────────────────────────────────
    e("UZ-STARTUP-001", .internal_server_error, "Environment check failed", "Required environment variables are missing. Run 'zombied doctor' to see which ones."),
    e("UZ-STARTUP-002", .internal_server_error, "Config load failed", "Configuration failed to load. Check that all required env vars are set. " ++
        "Run 'zombied doctor' to verify."),
    e("UZ-STARTUP-003", .internal_server_error, "Database connect failed", "Database is unreachable. Check that DATABASE_URL is set and the database accepts connections."),
    e("UZ-STARTUP-004", .internal_server_error, "Redis connect failed", "Redis is unreachable. Check that REDIS_URL_API is set " ++
        "and the Redis server accepts connections. Run 'zombied doctor' to verify."),
    e("UZ-STARTUP-005", .internal_server_error, "Migration check failed", "Database migration state could not be verified. Check DB connectivity."),
    e("UZ-STARTUP-006", .internal_server_error, "Startup env allocation failed", "An environment variable could not be allocated at startup (out of memory). " ++
        "A required secret fails the boot closed; optional config falls back to its default — check host memory pressure."),
    e("UZ-STARTUP-007", .internal_server_error, "Redis group creation failed", "Redis connected but consumer group creation failed. " ++
        "Check Redis ACL permissions allow XGROUP CREATE."),
    // ── RUNNER (zombie-runner /v1/runners control contract) ───────────────────
    e("UZ-RUN-001", .unauthorized, "Invalid runner token", "The Bearer runner_token is missing, malformed, or not recognized. Re-register the runner."),
    e("UZ-RUN-003", .bad_request, "Unsupported secret delivery mode", "The requested secret delivery mode is not supported. This deployment delivers secrets inline only."),
    e("UZ-RUN-005", .conflict, "Stale fencing token", "The lease was reclaimed by a newer holder. This report is rejected; the current holder's result wins."),
    e("UZ-RUN-006", .not_found, "Lease not found", "No active lease matches this lease_id for the presenting runner; it may have expired, been reclaimed, or never existed."),
    e("UZ-RUN-007", .internal_server_error, "Sandbox establishment failed", "The runner could not establish the required sandbox (Landlock/cgroup/netns) for execution; the lease was refused fail-closed rather than run unconfined."),
    e("UZ-RUN-009", .unauthorized, "Runner admin state blocks access", "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token."),
    e("UZ-RUN-010", .conflict, "Lease exceeded max runtime", "The lease reached the hard maximum runtime and may not be renewed further; the run is terminated. The child is killed and the result, if any, is reported."),
    e("UZ-RUN-011", .conflict, "Lease lost", "The lease was reassigned to another runner before this renewal; it can no longer be renewed. The presenting runner must terminate its child."),
    e("UZ-RUN-012", .payment_required, "Lease renewal blocked: no credits", "The tenant's balance can no longer cover continued execution; the lease may not be renewed and the run terminates gracefully."),
    e("UZ-RUN-013", .bad_request, "Renew body malformed", "The renew request body could not be parsed; cumulative token counts default to zero and the slice meters its run-time fee only (never a negative charge). The lease is still renewed."),
    e("UZ-RUN-014", .not_found, "Runner not found", "No runner matches this runner_id. Verify the platform admin minted the runner before mutating it."),
    // Runtime / execute-path entries (sandbox, runner, relay, credentials,
    // approval-gate, memory, api-keys, grants, tool/credential, proxy,
    // gate-execute) live in error_entries_runtime.zig and are concatenated
    // into REGISTRY by error_registry.zig.
};
