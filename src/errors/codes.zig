const std = @import("std");

pub const ERROR_DOCS_BASE = "https://docs.usezombie.com/error-codes#";

pub const ERR_UUIDV7_CANONICAL_FORMAT = "UZ-UUIDV7-003";
pub const ERR_UUIDV7_ID_GENERATION_FAILED = "UZ-UUIDV7-005";
pub const ERR_UUIDV7_INVALID_ID_SHAPE = "UZ-UUIDV7-009";
pub const ERR_UUIDV7_BACKFILL_CONFLICT = "UZ-UUIDV7-010";
pub const ERR_UUIDV7_ROLLBACK_BLOCKED = "UZ-UUIDV7-011";
pub const ERR_UUIDV7_ERROR_RESPONSE_LINKING = "UZ-UUIDV7-012";

pub const ERR_INTERNAL_DB_UNAVAILABLE = "UZ-INTERNAL-001";
pub const ERR_INTERNAL_DB_QUERY = "UZ-INTERNAL-002";
pub const ERR_INTERNAL_OPERATION_FAILED = "UZ-INTERNAL-003";

pub const ERR_INVALID_REQUEST = "UZ-REQ-001";
pub const ERR_PAYLOAD_TOO_LARGE = "UZ-REQ-002";
pub const ERR_FORBIDDEN = "UZ-AUTH-001";
pub const ERR_UNAUTHORIZED = "UZ-AUTH-002";
pub const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";
pub const ERR_AUTH_UNAVAILABLE = "UZ-AUTH-004";
pub const ERR_SESSION_NOT_FOUND = "UZ-AUTH-005";
pub const ERR_SESSION_EXPIRED = "UZ-AUTH-006";
pub const ERR_SESSION_ALREADY_COMPLETE = "UZ-AUTH-007";
pub const ERR_SESSION_LIMIT = "UZ-AUTH-008";
pub const ERR_INSUFFICIENT_ROLE = "UZ-AUTH-009";
pub const ERR_UNSUPPORTED_ROLE = "UZ-AUTH-010";

pub const ERR_API_SATURATED = "UZ-API-001";
pub const ERR_QUEUE_UNAVAILABLE = "UZ-API-002";
pub const ERR_WORKSPACE_NOT_FOUND = "UZ-WORKSPACE-001";
pub const ERR_WORKSPACE_PAUSED = "UZ-WORKSPACE-002";
pub const ERR_WORKSPACE_FREE_LIMIT = "UZ-WORKSPACE-003";
pub const ERR_BILLING_INVALID_SUBSCRIPTION_ID = "UZ-BILLING-001";
pub const ERR_BILLING_STATE_MISSING = "UZ-BILLING-002";
pub const ERR_BILLING_STATE_INVALID = "UZ-BILLING-003";
pub const ERR_BILLING_INVALID_EVENT = "UZ-BILLING-004";
pub const ERR_CREDIT_EXHAUSTED = "UZ-BILLING-005";
pub const ERR_SCORING_CONTEXT_TOKENS_INVALID = "UZ-SCORING-001";
pub const ERR_ENTITLEMENT_UNAVAILABLE = "UZ-ENTL-001";
pub const ERR_ENTITLEMENT_PROFILE_LIMIT = "UZ-ENTL-002";
pub const ERR_ENTITLEMENT_STAGE_LIMIT = "UZ-ENTL-003";
pub const ERR_ENTITLEMENT_SKILL_NOT_ALLOWED = "UZ-ENTL-004";
pub const ERR_SPEC_NOT_FOUND = "UZ-SPEC-001";
pub const ERR_SPEC_EMPTY = "UZ-SPEC-002";
pub const ERR_SPEC_NO_ACTIONABLE_CONTENT = "UZ-SPEC-003";
pub const ERR_SPEC_UNRESOLVED_FILE_REF = "UZ-SPEC-004";
pub const ERR_RUN_NOT_FOUND = "UZ-RUN-001";
pub const ERR_INVALID_STATE_TRANSITION = "UZ-RUN-002";
// M17_001: run budget and cancellation error codes
pub const ERR_RUN_TOKEN_BUDGET_EXCEEDED = "UZ-RUN-003";
pub const ERR_RUN_WALL_TIME_EXCEEDED = "UZ-RUN-004";
pub const ERR_WORKSPACE_MONTHLY_BUDGET_EXCEEDED = "UZ-RUN-005";
pub const ERR_RUN_ALREADY_TERMINAL = "UZ-RUN-006";
pub const ERR_RUN_CANCEL_SIGNAL_FAILED = "UZ-RUN-007";
pub const ERR_RUN_INTERRUPT_SIGNAL_FAILED = "UZ-RUN-008";
pub const ERR_RUN_NOT_INTERRUPTIBLE = "UZ-RUN-009";
pub const ERR_AGENT_NOT_FOUND = "UZ-AGENT-001";
pub const ERR_AGENT_SCORES_UNAVAILABLE = "UZ-AGENT-002";
pub const ERR_PROFILE_NOT_FOUND = "UZ-PROFILE-001";
pub const ERR_PROFILE_INVALID = "UZ-PROFILE-002";
pub const ERR_PROPOSAL_INVALID_JSON = "UZ-PROPOSAL-001";
pub const ERR_PROPOSAL_NOT_ARRAY = "UZ-PROPOSAL-002";
pub const ERR_PROPOSAL_CHANGE_NOT_OBJECT = "UZ-PROPOSAL-003";
pub const ERR_PROPOSAL_MISSING_TARGET_FIELD = "UZ-PROPOSAL-004";
pub const ERR_PROPOSAL_UNSUPPORTED_TARGET_FIELD = "UZ-PROPOSAL-005";
pub const ERR_PROPOSAL_MISSING_STAGE_ID = "UZ-PROPOSAL-006";
pub const ERR_PROPOSAL_MISSING_ROLE = "UZ-PROPOSAL-007";
pub const ERR_PROPOSAL_MISSING_INSERT_BEFORE_STAGE_ID = "UZ-PROPOSAL-008";
pub const ERR_PROPOSAL_DISALLOWED_FIELD = "UZ-PROPOSAL-009";
pub const ERR_PROPOSAL_UNREGISTERED_AGENT_REF = "UZ-PROPOSAL-010";
pub const ERR_PROPOSAL_INVALID_SKILL_REF = "UZ-PROPOSAL-011";
pub const ERR_PROPOSAL_UNKNOWN_STAGE_REF = "UZ-PROPOSAL-012";
pub const ERR_PROPOSAL_DUPLICATE_STAGE_REF = "UZ-PROPOSAL-013";
pub const ERR_PROPOSAL_WOULD_NOT_COMPILE = "UZ-PROPOSAL-014";
pub const ERR_PROPOSAL_NO_VALID_TEMPLATE = "UZ-PROPOSAL-015";
pub const ERR_PROPOSAL_GENERATION_FAILED = "UZ-PROPOSAL-016";
pub const ERR_PROPOSAL_NOT_FOUND = "UZ-PROPOSAL-017";
pub const ERR_HARNESS_CHANGE_NOT_FOUND = "UZ-HARNESS-001";

// M1_001: Zombie webhook error codes
pub const ERR_WEBHOOK_NO_ZOMBIE = "UZ-WH-001";
pub const ERR_WEBHOOK_MALFORMED = "UZ-WH-002";
pub const ERR_WEBHOOK_ZOMBIE_PAUSED = "UZ-WH-003";

// M1_001: Webhook user-facing error messages (rule 22: no inline strings)
pub const MSG_BODY_REQUIRED = "Request body required";
pub const MSG_MALFORMED_JSON = "Malformed JSON";
pub const MSG_MISSING_FIELDS = "event_id and type are required";
pub const MSG_ZOMBIE_NOT_FOUND = "Zombie not found";
pub const MSG_AUTH_REQUIRED = "Authorization required";
pub const MSG_BEARER_REQUIRED = "Bearer token required";
pub const MSG_INVALID_TOKEN = "Invalid token";
pub const MSG_ZOMBIE_NOT_ACTIVE = "Zombie is not active";

// Webhook constants
pub const BEARER_PREFIX = "Bearer ";
pub const DEDUP_TTL_SECONDS: u32 = 86400;
pub const ZOMBIE_STATUS_ACTIVE = "active";
pub const WEBHOOK_EVENT_TYPE = "webhook_received";
pub const STATUS_DUPLICATE = "duplicate";
pub const STATUS_ACCEPTED = "accepted";

// M1_001: Zombie event loop error codes
pub const ERR_ZOMBIE_BUDGET_EXCEEDED = "UZ-ZMB-001";
pub const ERR_ZOMBIE_AGENT_TIMEOUT = "UZ-ZMB-002";
pub const ERR_ZOMBIE_CREDENTIAL_MISSING = "UZ-ZMB-003";
pub const ERR_ZOMBIE_CLAIM_FAILED = "UZ-ZMB-004";
pub const ERR_ZOMBIE_CHECKPOINT_FAILED = "UZ-ZMB-005";

pub const ERR_GATE_COMMAND_FAILED = "UZ-GATE-001";
pub const ERR_GATE_COMMAND_TIMEOUT = "UZ-GATE-002";
pub const ERR_GATE_REPAIR_EXHAUSTED = "UZ-GATE-003";
pub const ERR_GATE_PERSIST_FAILED = "UZ-GATE-004";

pub const ERR_STARTUP_ENV_CHECK = "UZ-STARTUP-001";
pub const ERR_STARTUP_CONFIG_LOAD = "UZ-STARTUP-002";
pub const ERR_STARTUP_DB_CONNECT = "UZ-STARTUP-003";
pub const ERR_STARTUP_REDIS_CONNECT = "UZ-STARTUP-004";
pub const ERR_STARTUP_REDIS_GROUP = "UZ-STARTUP-007";
pub const ERR_STARTUP_MIGRATION_CHECK = "UZ-STARTUP-005";
pub const ERR_STARTUP_OIDC_INIT = "UZ-STARTUP-006";
pub const ERR_SANDBOX_BACKEND_UNAVAILABLE = "UZ-SANDBOX-001";
pub const ERR_SANDBOX_KILL_SWITCH_TRIGGERED = "UZ-SANDBOX-002";
pub const ERR_SANDBOX_COMMAND_BLOCKED = "UZ-SANDBOX-003";

pub const ERR_WORKER_PROMPTS_LOAD = "UZ-WORKER-001";
pub const ERR_WORKER_PROFILE_INIT = "UZ-WORKER-002";

pub const ERR_EXEC_SESSION_CREATE_FAILED = "UZ-EXEC-001";
pub const ERR_EXEC_STAGE_START_FAILED = "UZ-EXEC-002";
pub const ERR_EXEC_TIMEOUT_KILL = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL = "UZ-EXEC-004";
pub const ERR_EXEC_RESOURCE_KILL = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS = "UZ-EXEC-006";
pub const ERR_EXEC_LEASE_EXPIRED = "UZ-EXEC-007";
pub const ERR_EXEC_POLICY_DENY = "UZ-EXEC-008";
pub const ERR_EXEC_STARTUP_POSTURE = "UZ-EXEC-009";
pub const ERR_EXEC_CRASH = "UZ-EXEC-010";
pub const ERR_EXEC_LANDLOCK_DENY = "UZ-EXEC-011";
pub const ERR_EXEC_RUNNER_AGENT_INIT = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_AGENT_RUN = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG = "UZ-EXEC-014";

// M18_003: agent relay error codes
pub const ERR_RELAY_NO_PROVIDER = "UZ-RELAY-001";

pub const ERR_CRED_ANTHROPIC_KEY_MISSING = "UZ-CRED-001";
pub const ERR_CRED_GITHUB_TOKEN_FAILED = "UZ-CRED-002";
pub const ERR_CRED_PLATFORM_KEY_MISSING = "UZ-CRED-003";

pub fn docsRef(code: []const u8) struct { base: []const u8, code: []const u8 } {
    return .{
        .base = ERROR_DOCS_BASE,
        .code = code,
    };
}

/// Returns an actionable hint for a given error code, or null if none defined.
/// Hints tell the operator (or admin) what to check or do next — git-style.
pub fn hint(code: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, code, ERR_INTERNAL_DB_UNAVAILABLE))
        return "Check that the DATABASE_URL is set and the database server is reachable. Run 'zombied doctor' to verify.";
    if (std.mem.eql(u8, code, ERR_INTERNAL_OPERATION_FAILED))
        return "An internal operation failed. Check the err= field for details. If persistent, check service connectivity and run 'zombied doctor'.";
    if (std.mem.eql(u8, code, ERR_STARTUP_REDIS_CONNECT))
        return "Redis is unreachable. Check that REDIS_URL_API and REDIS_URL_WORKER are set and the Redis server accepts connections. Run 'zombied doctor' to verify.";
    if (std.mem.eql(u8, code, ERR_STARTUP_REDIS_GROUP))
        return "Redis connected but consumer group creation failed. Check Redis ACL permissions allow XGROUP CREATE, or check server logs for errors.";
    if (std.mem.eql(u8, code, ERR_STARTUP_DB_CONNECT))
        return "Database is unreachable. Check that DATABASE_URL is set and the database server accepts connections.";
    if (std.mem.eql(u8, code, ERR_STARTUP_CONFIG_LOAD))
        return "Configuration failed to load. Check that all required environment variables are set. Run 'zombied doctor' to verify.";
    if (std.mem.eql(u8, code, ERR_STARTUP_ENV_CHECK))
        return "Required environment variables are missing. Run 'zombied doctor' to see which ones.";
    if (std.mem.eql(u8, code, ERR_WORKER_PROMPTS_LOAD))
        return "Agent prompt files not found. Check that AGENT_CONFIG_DIR points to a directory containing echo-prompt.md, scout-prompt.md, and warden-prompt.md.";
    if (std.mem.eql(u8, code, ERR_WORKER_PROFILE_INIT))
        return "Pipeline profile failed to initialize. Check that PIPELINE_PROFILE_PATH points to a valid JSON profile, or remove it to use the default.";
    if (std.mem.eql(u8, code, ERR_SANDBOX_BACKEND_UNAVAILABLE))
        return "Sandbox backend is not available. Check that bubblewrap (bwrap) is installed and accessible.";
    if (std.mem.eql(u8, code, ERR_GATE_COMMAND_FAILED))
        return "A gate command (make lint/test/build) failed. Check the gate results for stdout/stderr output.";
    if (std.mem.eql(u8, code, ERR_GATE_COMMAND_TIMEOUT))
        return "A gate command exceeded its timeout. Increase GATE_TOOL_TIMEOUT_MS or optimize the gate command.";
    if (std.mem.eql(u8, code, ERR_GATE_REPAIR_EXHAUSTED))
        return "Agent exhausted all repair attempts without passing gates. Review gate results for the repeated failure pattern.";
    if (std.mem.eql(u8, code, ERR_GATE_PERSIST_FAILED))
        return "Gate results could not be written to the database. Check DB connectivity and that the gate_results table exists.";
    if (std.mem.eql(u8, code, ERR_CRED_ANTHROPIC_KEY_MISSING))
        return "Workspace LLM API key not found in vault.secrets (key: anthropic_api_key). Set it via the workspace credentials API. Executor fell back to its process environment — check ANTHROPIC_API_KEY on the worker if this is dev mode.";
    if (std.mem.eql(u8, code, ERR_CRED_GITHUB_TOKEN_FAILED))
        return "GitHub App installation token request failed. Check GITHUB_APP_ID and GITHUB_APP_PRIVATE_KEY, verify the installation_id is valid, and inspect GitHub API status.";
    if (std.mem.eql(u8, code, ERR_CRED_PLATFORM_KEY_MISSING))
        return "No active platform LLM key for this provider. Admin must set one via PUT /v1/admin/platform-keys, or the workspace must add its own key via PUT /v1/workspaces/{id}/credentials/llm.";
    if (std.mem.eql(u8, code, ERR_RUN_TOKEN_BUDGET_EXCEEDED))
        return "Run exceeded its token budget (max_tokens). The run has been terminated. Adjust max_tokens in the agent profile or reduce prompt complexity.";
    if (std.mem.eql(u8, code, ERR_RUN_WALL_TIME_EXCEEDED))
        return "Run exceeded its wall-clock time limit (max_wall_time_seconds). The run has been terminated. Increase the limit in the agent profile or reduce workload.";
    if (std.mem.eql(u8, code, ERR_WORKSPACE_MONTHLY_BUDGET_EXCEEDED))
        return "Workspace monthly token budget exhausted. Upgrade the workspace plan or wait for the next calendar month to reset the budget.";
    if (std.mem.eql(u8, code, ERR_RUN_ALREADY_TERMINAL))
        return "The run is already in a terminal state (DONE, BLOCKED, CANCELLED). No further state changes are allowed.";
    if (std.mem.eql(u8, code, ERR_RUN_CANCEL_SIGNAL_FAILED))
        return "Cancel signal could not be published. Check Redis connectivity and retry.";
    if (std.mem.eql(u8, code, ERR_RUN_INTERRUPT_SIGNAL_FAILED))
        return "Interrupt message could not be stored in Redis. Check Redis connectivity and retry.";
    if (std.mem.eql(u8, code, ERR_RUN_NOT_INTERRUPTIBLE))
        return "Run is not in an interruptible state. Check the run state and retry.";
    if (std.mem.eql(u8, code, ERR_APPROVAL_PARSE_FAILED))
        return "Gate policy in TRIGGER.md config_json has invalid syntax. Check the 'gates' section for valid JSON structure.";
    if (std.mem.eql(u8, code, ERR_APPROVAL_NOT_FOUND))
        return "Approval action not found or already resolved. The action may have timed out or been handled by another click.";
    if (std.mem.eql(u8, code, ERR_APPROVAL_REDIS_UNAVAILABLE))
        return "Gate service unavailable — default-deny applied. Check Redis connectivity.";
    if (std.mem.eql(u8, code, ERR_APPROVAL_CONDITION_INVALID))
        return "Gate condition expression is invalid. Supported operators: == and != with single-quoted string values.";
    if (std.mem.eql(u8, code, ERR_ZOMBIE_BUDGET_EXCEEDED))
        return "Zombie hit its daily budget. Increase with: zombiectl config set budget.daily_dollars <amount>";
    if (std.mem.eql(u8, code, ERR_ZOMBIE_AGENT_TIMEOUT))
        return "Agent timed out processing an event. Check activity stream for details: zombiectl logs";
    if (std.mem.eql(u8, code, ERR_ZOMBIE_CREDENTIAL_MISSING))
        return "A required credential is not in the vault. Add it with: zombiectl credential add <name>";
    if (std.mem.eql(u8, code, ERR_ZOMBIE_CLAIM_FAILED))
        return "Zombie could not be claimed from the database. Check that the zombie_id exists and status is 'active'.";
    if (std.mem.eql(u8, code, ERR_ZOMBIE_CHECKPOINT_FAILED))
        return "Session checkpoint write to Postgres failed. Check database connectivity.";
    return null;
}

test "docsRef returns single base and code tuple" {
    const ref = docsRef(ERR_UUIDV7_INVALID_ID_SHAPE);
    try std.testing.expectEqualStrings(ERROR_DOCS_BASE, ref.base);
    try std.testing.expectEqualStrings("UZ-UUIDV7-009", ref.code);
}

test "hint returns actionable text for known startup codes" {
    try std.testing.expect(hint(ERR_STARTUP_REDIS_CONNECT) != null);
    try std.testing.expect(hint(ERR_WORKER_PROMPTS_LOAD) != null);
    try std.testing.expect(hint(ERR_WORKER_PROFILE_INIT) != null);
    try std.testing.expect(hint(ERR_INTERNAL_DB_UNAVAILABLE) != null);
}

test "hint returns null for codes without hints" {
    try std.testing.expectEqual(@as(?[]const u8, null), hint(ERR_UUIDV7_CANONICAL_FORMAT));
    try std.testing.expectEqual(@as(?[]const u8, null), hint(ERR_RUN_NOT_FOUND));
}

// M4_001: Approval gate error codes
pub const ERR_APPROVAL_PARSE_FAILED = "UZ-APPROVAL-001";
pub const ERR_APPROVAL_NOT_FOUND = "UZ-APPROVAL-002";
pub const ERR_APPROVAL_INVALID_SIGNATURE = "UZ-APPROVAL-003";
pub const ERR_APPROVAL_REDIS_UNAVAILABLE = "UZ-APPROVAL-004";
pub const ERR_APPROVAL_CONDITION_INVALID = "UZ-APPROVAL-005";

// M4_001: Approval gate user-facing messages
pub const MSG_APPROVAL_NOT_FOUND = "Approval action not found or already resolved";
pub const MSG_APPROVAL_INVALID_BODY = "Invalid approval payload";
pub const MSG_APPROVAL_INVALID_DECISION = "Decision must be 'approve' or 'deny'";

// M4_001: Approval gate constants
pub const GATE_DEFAULT_TIMEOUT_MS: u64 = 3_600_000; // 1 hour
pub const GATE_ANOMALY_KEY_PREFIX = "zombie:anomaly:";
pub const GATE_PENDING_KEY_PREFIX = "zombie:gate:pending:";
pub const GATE_RESPONSE_KEY_PREFIX = "zombie:gate:response:";
pub const GATE_PENDING_TTL_SECONDS: u32 = 7200; // 2h (covers 1h timeout + buffer)
pub const GATE_DECISION_APPROVE = "approve";
pub const GATE_DECISION_DENY = "deny";

// M4_001: Gate activity event types
pub const GATE_EVENT_REQUIRED = "gate_approval_required";
pub const GATE_EVENT_APPROVED = "gate_approved";
pub const GATE_EVENT_DENIED = "gate_denied";
pub const GATE_EVENT_TIMEOUT = "gate_timeout";
pub const GATE_EVENT_AUTO_KILL = "gate_auto_kill";
pub const GATE_EVENT_AUTO_APPROVE = "gate_auto_approve";

// ── T8 — OWASP Agent Security: credential error codes (M16_003) ──────

// T8.1 — M16_003/M16_004 credential error codes exist and follow UZ-CRED- naming.
test "T8: ERR_CRED_* codes follow UZ-CRED- prefix naming (M16_003/M16_004)" {
    try std.testing.expect(std.mem.startsWith(u8, ERR_CRED_ANTHROPIC_KEY_MISSING, "UZ-CRED-"));
    try std.testing.expect(std.mem.startsWith(u8, ERR_CRED_GITHUB_TOKEN_FAILED, "UZ-CRED-"));
    try std.testing.expect(std.mem.startsWith(u8, ERR_CRED_PLATFORM_KEY_MISSING, "UZ-CRED-"));
}

// T8.2 — All credential codes have distinct values (no code collision).
test "T8: ERR_CRED_* codes are distinct — no collision (M16_003/M16_004)" {
    try std.testing.expect(!std.mem.eql(u8, ERR_CRED_ANTHROPIC_KEY_MISSING, ERR_CRED_GITHUB_TOKEN_FAILED));
    try std.testing.expect(!std.mem.eql(u8, ERR_CRED_ANTHROPIC_KEY_MISSING, ERR_CRED_PLATFORM_KEY_MISSING));
    try std.testing.expect(!std.mem.eql(u8, ERR_CRED_GITHUB_TOKEN_FAILED, ERR_CRED_PLATFORM_KEY_MISSING));
}

// T8.3 — Credential error hints exist and are actionable but do not expose raw secret values.
// The hint must tell the operator WHERE to look (vault item name, env var name) but must
// never contain a literal credential value, a resolved secret, or a raw token prefix.
test "T8: credential error hints are actionable and contain no raw secret values (M16_003)" {
    const anthropic_hint = hint(ERR_CRED_ANTHROPIC_KEY_MISSING).?;
    try std.testing.expect(anthropic_hint.len > 0);
    // Must NOT contain a literal Anthropic key prefix (sk-ant- is a live credential prefix).
    try std.testing.expect(std.mem.indexOf(u8, anthropic_hint, "sk-ant-api") == null);
    // Must NOT contain a literal Bearer token header value.
    try std.testing.expect(std.mem.indexOf(u8, anthropic_hint, "Bearer ") == null);

    const github_hint = hint(ERR_CRED_GITHUB_TOKEN_FAILED).?;
    try std.testing.expect(github_hint.len > 0);
    // Must NOT contain a raw GitHub installation token prefix.
    try std.testing.expect(std.mem.indexOf(u8, github_hint, "ghs_") == null);
    try std.testing.expect(std.mem.indexOf(u8, github_hint, "ghp_") == null);
}

// T8.4 — ERR_CRED_GITHUB_TOKEN_FAILED hint references GitHub App auth (not PAT fallback).
// Guards against a hint that silently suggests a less-secure auth method.
test "T8: ERR_CRED_GITHUB_TOKEN_FAILED hint references GitHub App — no PAT fallback (M16_003)" {
    const h = hint(ERR_CRED_GITHUB_TOKEN_FAILED).?;
    // Must mention GitHub App concepts (GITHUB_APP_ID, private key, or installation).
    const mentions_app = std.mem.indexOf(u8, h, "App") != null or
        std.mem.indexOf(u8, h, "installation") != null or
        std.mem.indexOf(u8, h, "GITHUB_APP") != null;
    try std.testing.expect(mentions_app);
}

// ── T8 M16_004 — platform key error code ────────────────────────────────────

// T8.5 — ERR_CRED_PLATFORM_KEY_MISSING hint is actionable and names the admin endpoint.
test "T8: ERR_CRED_PLATFORM_KEY_MISSING hint names admin endpoint and BYOK path (M16_004)" {
    const h = hint(ERR_CRED_PLATFORM_KEY_MISSING).?;
    try std.testing.expect(h.len > 0);
    // Must reference the admin API path so operators know where to go.
    const mentions_admin = std.mem.indexOf(u8, h, "platform-keys") != null or
        std.mem.indexOf(u8, h, "admin") != null;
    try std.testing.expect(mentions_admin);
    // Must reference the workspace BYOK path as the alternative.
    const mentions_byok = std.mem.indexOf(u8, h, "credentials/llm") != null or
        std.mem.indexOf(u8, h, "BYOK") != null or
        std.mem.indexOf(u8, h, "own key") != null;
    try std.testing.expect(mentions_byok);
    // Must not expose any raw key material.
    try std.testing.expect(std.mem.indexOf(u8, h, "sk-ant-") == null);
    try std.testing.expect(std.mem.indexOf(u8, h, "Bearer ") == null);
}
