/// error_registry.zig — comptime-generated error registry.
///
/// Single source of truth for error codes. Adding a new error code:
/// 1. Add one e() entry to ENTRIES in error_entries.zig (control-plane)
///    or ENTRIES_RUNTIME in error_entries_runtime.zig (execute path).
/// 2. Add the ERR_* constant below.
/// Comptime validation guarantees: non-empty hints, UZ- prefix, no duplicates,
/// no sentinel collision, and every ERR_* resolves in the registry.
const std = @import("std");
const entries = @import("error_entries.zig");
const entries_runtime = @import("error_entries_runtime.zig");

pub const Entry = entries.Entry;
pub const UNKNOWN = entries.UNKNOWN;
pub const ERROR_DOCS_BASE = entries.ERROR_DOCS_BASE;
pub const REGISTRY = entries.ENTRIES ++ entries_runtime.ENTRIES_RUNTIME;

// ── Comptime validation ────────────────────────────────────────────────────
comptime {
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 20);
    for (REGISTRY) |entry| {
        if (entry.hint.len == 0)
            @compileError("Entry has empty hint: " ++ entry.code);
        if (entry.code.len < 4 or !std.mem.startsWith(u8, entry.code, "UZ-"))
            @compileError("Entry code must start with UZ-: " ++ entry.code);
    }
    // Invariant 3: no sentinel collision
    for (REGISTRY) |entry| {
        if (std.mem.eql(u8, entry.code, UNKNOWN.code))
            @compileError("REGISTRY entry collides with UNKNOWN sentinel: " ++ entry.code);
    }
    // Invariant 5: no duplicate codes
    for (REGISTRY, 0..) |a, i| {
        for (REGISTRY[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a.code, b.code))
                @compileError("Duplicate code in REGISTRY: " ++ a.code);
        }
    }
}

// ── Lookup ─────────────────────────────────────────────────────────────────
const LOOKUP = blk: {
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 20);
    var kvs: [REGISTRY.len]struct { []const u8, usize } = undefined;
    for (REGISTRY, 0..) |entry, i| kvs[i] = .{ entry.code, i };
    break :blk std.StaticStringMap(usize).initComptime(kvs);
};

/// Lookup by code string. Returns UNKNOWN for unregistered codes.
/// Never returns null — callers do not need optional handling.
pub fn lookup(code: []const u8) Entry {
    const idx = LOOKUP.get(code) orelse return UNKNOWN;
    return REGISTRY[idx];
}

/// Lookup hint for an error code. Returns UNKNOWN.hint for unregistered codes.
pub fn hint(code: []const u8) []const u8 {
    return lookup(code).hint;
}

// ── ERR_* constants ────────────────────────────────────────────────────────
// UUIDV7
const ERR_UUIDV7_CANONICAL_FORMAT = "UZ-UUIDV7-003";
const ERR_UUIDV7_ID_GENERATION_FAILED = "UZ-UUIDV7-005";
pub const ERR_UUIDV7_INVALID_ID_SHAPE = "UZ-UUIDV7-009";
const ERR_UUIDV7_BACKFILL_CONFLICT = "UZ-UUIDV7-010";
const ERR_UUIDV7_ROLLBACK_BLOCKED = "UZ-UUIDV7-011";
const ERR_UUIDV7_ERROR_RESPONSE_LINKING = "UZ-UUIDV7-012";
// INTERNAL
pub const ERR_INTERNAL_DB_UNAVAILABLE = "UZ-INTERNAL-001";
pub const ERR_INTERNAL_DB_QUERY = "UZ-INTERNAL-002";
pub const ERR_INTERNAL_OPERATION_FAILED = "UZ-INTERNAL-003";
// REQUEST
pub const ERR_INVALID_REQUEST = "UZ-REQ-001";
pub const ERR_PAYLOAD_TOO_LARGE = "UZ-REQ-002";
// AUTH
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
// API
const ERR_API_SATURATED = "UZ-API-001";
const ERR_QUEUE_UNAVAILABLE = "UZ-API-002";
// WORKSPACE
pub const ERR_WORKSPACE_NOT_FOUND = "UZ-WORKSPACE-001";
pub const ERR_WORKSPACE_PAUSED = "UZ-WORKSPACE-002";
const ERR_WORKSPACE_FREE_LIMIT = "UZ-WORKSPACE-003";
// BILLING
const ERR_BILLING_INVALID_SUBSCRIPTION_ID = "UZ-BILLING-001";
const ERR_BILLING_STATE_MISSING = "UZ-BILLING-002";
const ERR_BILLING_STATE_INVALID = "UZ-BILLING-003";
const ERR_BILLING_INVALID_EVENT = "UZ-BILLING-004";
pub const ERR_CREDIT_EXHAUSTED = "UZ-BILLING-005";
// SCORING
const ERR_SCORING_CONTEXT_TOKENS_INVALID = "UZ-SCORING-001";
// ENTITLEMENT
const ERR_ENTITLEMENT_UNAVAILABLE = "UZ-ENTL-001";
const ERR_ENTITLEMENT_STAGE_LIMIT = "UZ-ENTL-003";
const ERR_ENTITLEMENT_SKILL_NOT_ALLOWED = "UZ-ENTL-004";
// AGENT
pub const ERR_AGENT_NOT_FOUND = "UZ-AGENT-001";
// PROFILE
const ERR_PROFILE_NOT_FOUND = "UZ-PROFILE-001";
const ERR_PROFILE_INVALID = "UZ-PROFILE-002";
// WEBHOOK
pub const ERR_WEBHOOK_NO_ZOMBIE = "UZ-WH-001";
pub const ERR_WEBHOOK_MALFORMED = "UZ-WH-002";
pub const ERR_WEBHOOK_ZOMBIE_PAUSED = "UZ-WH-003";
pub const ERR_WEBHOOK_SIG_INVALID = "UZ-WH-010";
pub const ERR_WEBHOOK_TIMESTAMP_STALE = "UZ-WH-011";
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = "UZ-WH-020";
pub const ERR_WEBHOOK_PAYLOAD_TOO_LARGE = "UZ-WH-030";
// TOOL
const ERR_TOOL_CREDENTIAL_MISSING = "UZ-TOOL-001";
const ERR_TOOL_API_FAILED = "UZ-TOOL-002";
const ERR_TOOL_GIT_FAILED = "UZ-TOOL-003";
const ERR_TOOL_NOT_ATTACHED = "UZ-TOOL-004";
pub const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";
const ERR_TOOL_TIMEOUT = "UZ-TOOL-006";
// ZOMBIE
pub const ERR_ZOMBIE_BUDGET_EXCEEDED = "UZ-ZMB-001";
pub const ERR_ZOMBIE_AGENT_TIMEOUT = "UZ-ZMB-002";
pub const ERR_ZOMBIE_CREDENTIAL_MISSING = "UZ-ZMB-003";
pub const ERR_ZOMBIE_CLAIM_FAILED = "UZ-ZMB-004";
pub const ERR_ZOMBIE_CHECKPOINT_FAILED = "UZ-ZMB-005";
pub const ERR_ZOMBIE_NAME_EXISTS = "UZ-ZMB-006";
// UZ-ZMB-007 retired — superseded by UZ-VAULT-002 (credential data too large).
pub const ERR_ZOMBIE_INVALID_CONFIG = "UZ-ZMB-008";
pub const ERR_ZOMBIE_NOT_FOUND = "UZ-ZMB-009";
pub const ERR_ZOMBIE_ALREADY_TERMINAL = "UZ-ZMB-010";
pub const ERR_ZOMBIE_NAME_MISMATCH = "UZ-ZMB-011";
// VAULT (structured-credential JSON shape)
pub const ERR_VAULT_DATA_INVALID = "UZ-VAULT-001";
pub const ERR_VAULT_DATA_TOO_LARGE = "UZ-VAULT-002";
// PROVIDER (tenant-scoped LLM provider config — PUT /v1/tenants/me/provider)
pub const ERR_PROVIDER_CREDENTIAL_REF_REQUIRED = "UZ-PROVIDER-001";
pub const ERR_PROVIDER_CREDENTIAL_NOT_FOUND = "UZ-PROVIDER-002";
pub const ERR_PROVIDER_CREDENTIAL_DATA_MALFORMED = "UZ-PROVIDER-003";
pub const ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE = "UZ-PROVIDER-004";
// MEMORY
pub const ERR_MEM_SCOPE = "UZ-MEM-001";
pub const ERR_MEM_ZOMBIE_NOT_FOUND = "UZ-MEM-002";
pub const ERR_MEM_UNAVAILABLE = "UZ-MEM-003";
// GATE
pub const ERR_GATE_COMMAND_FAILED = "UZ-GATE-001";
pub const ERR_GATE_COMMAND_TIMEOUT = "UZ-GATE-002";
pub const ERR_GATE_REPAIR_EXHAUSTED = "UZ-GATE-003";
const ERR_GATE_PERSIST_FAILED = "UZ-GATE-004";
// STARTUP
pub const ERR_STARTUP_ENV_CHECK = "UZ-STARTUP-001";
pub const ERR_STARTUP_CONFIG_LOAD = "UZ-STARTUP-002";
pub const ERR_STARTUP_DB_CONNECT = "UZ-STARTUP-003";
pub const ERR_STARTUP_REDIS_CONNECT = "UZ-STARTUP-004";
pub const ERR_STARTUP_MIGRATION_CHECK = "UZ-STARTUP-005";
const ERR_STARTUP_OIDC_INIT = "UZ-STARTUP-006";
pub const ERR_STARTUP_REDIS_GROUP = "UZ-STARTUP-007";
// SANDBOX
const ERR_SANDBOX_BACKEND_UNAVAILABLE = "UZ-SANDBOX-001";
const ERR_SANDBOX_KILL_SWITCH_TRIGGERED = "UZ-SANDBOX-002";
const ERR_SANDBOX_COMMAND_BLOCKED = "UZ-SANDBOX-003";
// EXECUTOR
pub const ERR_EXEC_SESSION_CREATE_FAILED = "UZ-EXEC-001";
pub const ERR_EXEC_STAGE_START_FAILED = "UZ-EXEC-002";
pub const ERR_EXEC_TIMEOUT_KILL = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL = "UZ-EXEC-004";
const ERR_EXEC_RESOURCE_KILL = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS = "UZ-EXEC-006";
const ERR_EXEC_LEASE_EXPIRED = "UZ-EXEC-007";
pub const ERR_EXEC_STARTUP_POSTURE = "UZ-EXEC-009";
const ERR_EXEC_CRASH = "UZ-EXEC-010";
const ERR_EXEC_LANDLOCK_DENY = "UZ-EXEC-011";
pub const ERR_EXEC_RUNNER_AGENT_INIT = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_AGENT_RUN = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG = "UZ-EXEC-014";
// RELAY
const ERR_RELAY_NO_PROVIDER = "UZ-RELAY-001";
// CREDENTIALS
pub const ERR_CRED_ANTHROPIC_KEY_MISSING = "UZ-CRED-001";
pub const ERR_CRED_PLATFORM_KEY_MISSING = "UZ-CRED-003";
// APPROVAL
pub const ERR_APPROVAL_PARSE_FAILED = "UZ-APPROVAL-001";
pub const ERR_APPROVAL_NOT_FOUND = "UZ-APPROVAL-002";
pub const ERR_APPROVAL_INVALID_SIGNATURE = "UZ-APPROVAL-003";
pub const ERR_APPROVAL_REDIS_UNAVAILABLE = "UZ-APPROVAL-004";
pub const ERR_APPROVAL_CONDITION_INVALID = "UZ-APPROVAL-005";
pub const ERR_APPROVAL_ALREADY_RESOLVED = "UZ-APPROVAL-006";
pub const ERR_APIKEY_INVALID = "UZ-APIKEY-001";
const ERR_APIKEY_PERMISSION = "UZ-APIKEY-002";
pub const ERR_APIKEY_NOT_FOUND = "UZ-APIKEY-003";
pub const ERR_APIKEY_REVOKED = "UZ-APIKEY-004";
pub const ERR_APIKEY_NAME_TAKEN = "UZ-APIKEY-005";
pub const ERR_APIKEY_ALREADY_REVOKED = "UZ-APIKEY-006";
pub const ERR_APIKEY_READONLY_FIELD = "UZ-APIKEY-007";
pub const ERR_APIKEY_MUST_REVOKE_FIRST = "UZ-APIKEY-008";
pub const ERR_GRANT_NOT_FOUND = "UZ-GRANT-001";
pub const ERR_GRANT_PENDING = "UZ-GRANT-002";
pub const ERR_GRANT_DENIED = "UZ-GRANT-003";

// ── Error mapping table (bvisor pattern) ─────────────────────────────────────
// Shared type for modules that map Zig errors to registry codes + messages.
// Use with `inline for` over a const table for comptime-validated dispatch.
pub const ErrorMapping = struct {
    err: anyerror,
    code: []const u8,
    message: []const u8,
};

/// Comptime-validate an error mapping table: no empty codes/messages, no duplicate errors or codes.
pub fn validateErrorTable(comptime table: []const ErrorMapping) void {
    for (table) |entry| {
        if (entry.code.len == 0) @compileError("error table: empty code");
        if (entry.message.len == 0) @compileError("error table: empty message");
    }
    for (table, 0..) |a, i| {
        for (table[i + 1 ..]) |b| {
            if (a.err == b.err) @compileError("error table: duplicate error");
            if (std.mem.eql(u8, a.code, b.code)) @compileError("error table: duplicate code " ++ a.code);
        }
    }
}

// ── Non-error constants (migrated from codes.zig) ──────────────────────────
// Webhook user-facing messages
pub const MSG_BODY_REQUIRED = "Request body required";
pub const MSG_MALFORMED_JSON = "Malformed JSON";
pub const MSG_MISSING_FIELDS = "event_id and type are required";
pub const MSG_ZOMBIE_NOT_FOUND = "Zombie not found";
const MSG_AUTH_REQUIRED = "Authorization required";
const MSG_BEARER_REQUIRED = "Bearer token required";
const MSG_INVALID_TOKEN = "Invalid token";
pub const MSG_ZOMBIE_NOT_ACTIVE = "Zombie is not active";
// Zombie CRUD messages
pub const MSG_ZOMBIE_NAME_EXISTS = "Zombie already exists in this workspace. Use `zombiectl kill` first.";
pub const MSG_ZOMBIE_INVALID_CONFIG = "Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.";
pub const MSG_ZOMBIE_NAME_MISMATCH = "SKILL.md `name:` must match TRIGGER.md `name:`.";
pub const MSG_ZOMBIE_SKILL_INVALID = "SKILL.md frontmatter is invalid. Required: name (kebab, 1-64 chars), description, version (semver MAJOR.MINOR.PATCH).";
pub const MSG_ZOMBIE_NAME_REQUIRED = "name is required (max 64 chars, slug-safe)";
pub const MSG_ZOMBIE_SOURCE_REQUIRED = "source_markdown is required (max 64KB)";
pub const MSG_ZOMBIE_TRIGGER_REQUIRED = "trigger_markdown is required (max 64KB)";
pub const MSG_ZOMBIE_CONFIG_REQUIRED = "config_json is required";
pub const MSG_WORKSPACE_ID_REQUIRED = "workspace_id is required (UUIDv7)";
pub const MSG_CREDENTIAL_NAME_REQUIRED = "credential name is required (max 64 chars)";
pub const MSG_CREDENTIAL_DATA_REQUIRED = "credential data must be a non-empty JSON object";
pub const MSG_CREDENTIAL_DATA_TOO_LARGE = "credential data exceeds 4KB when stringified";
// Approval messages
pub const MSG_APPROVAL_NOT_FOUND = "Approval action not found or already resolved";
pub const MSG_APPROVAL_INVALID_BODY = "Invalid approval payload";
pub const MSG_APPROVAL_INVALID_DECISION = "Decision must be 'approve' or 'deny'";
// Webhook signature messages
const MSG_WEBHOOK_SIG_INVALID = "Webhook signature verification failed. Check signing secret.";
const MSG_WEBHOOK_TS_STALE = "Webhook request too old (>5 min). Replay attack rejected.";
// Webhook constants
pub const BEARER_PREFIX = "Bearer ";
pub const DEDUP_TTL_SECONDS: u32 = 86400;
const ZOMBIE_STATUS_ACTIVE = "active";
const ZOMBIE_STATUS_PAUSED = "paused";
const ZOMBIE_STATUS_STOPPED = "stopped";
pub const WEBHOOK_EVENT_TYPE = "webhook_received";
pub const STATUS_DUPLICATE = "duplicate";
pub const STATUS_ACCEPTED = "accepted";
// Slack signature constants
pub const SLACK_SIG_VERSION = "v0";
pub const SLACK_SIG_HEADER = "x-slack-signature";
pub const SLACK_TS_HEADER = "x-slack-request-timestamp";
pub const SLACK_MAX_TS_DRIFT_SECONDS: i64 = 300;
// Gate constants
pub const GATE_DEFAULT_TIMEOUT_MS: u64 = 3_600_000;
pub const GATE_ANOMALY_KEY_PREFIX = "zombie:anomaly:";
pub const GATE_PENDING_KEY_PREFIX = "zombie:gate:pending:";
pub const GATE_RESPONSE_KEY_PREFIX = "zombie:gate:response:";
pub const GATE_PENDING_TTL_SECONDS: u32 = 7200;
pub const GATE_DECISION_APPROVE = "approve";
pub const GATE_DECISION_DENY = "deny";
// Gate activity event types
pub const GATE_EVENT_REQUIRED = "gate_approval_required";
pub const GATE_EVENT_APPROVED = "gate_approved";
pub const GATE_EVENT_DENIED = "gate_denied";
pub const GATE_EVENT_TIMEOUT = "gate_timeout";
pub const GATE_EVENT_AUTO_KILL = "gate_auto_kill";
pub const GATE_EVENT_AUTO_APPROVE = "gate_auto_approve";

// ── Comptime self-check: every ERR_* constant exists in REGISTRY ───────────
comptime {
    @setEvalBranchQuota(1_000_000);
    const decls = @typeInfo(@This()).@"struct".decls;
    for (decls) |decl| {
        if (std.mem.startsWith(u8, decl.name, "ERR_")) {
            const code: []const u8 = @field(@This(), decl.name);
            if (LOOKUP.get(code) == null) {
                @compileError("ERR_* constant not in REGISTRY: " ++ code);
            }
        }
    }
}

test {
    _ = @import("codes_test.zig");
    _ = @import("error_registry_test.zig");
}
