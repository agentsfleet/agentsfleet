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

const EVAL_BRANCH_QUOTA = 1_000_000;

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
pub const ERR_UUIDV7_INVALID_ID_SHAPE = "UZ-UUIDV7-009";
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
pub const ERR_VERIFICATION_FAILED = "UZ-AUTH-011";
pub const ERR_SESSION_CONSUMED = "UZ-AUTH-012";
pub const ERR_SESSION_ABORTED = "UZ-AUTH-013";
pub const ERR_SESSION_NOT_APPROVED = "UZ-AUTH-014";
pub const ERR_SESSION_ALREADY_APPROVED = "UZ-AUTH-015";
pub const ERR_INVALID_PUBLIC_KEY = "UZ-AUTH-016";
pub const ERR_INVALID_TOKEN_NAME = "UZ-AUTH-017";
pub const ERR_INVALID_VERIFICATION_CODE = "UZ-AUTH-018";
pub const ERR_INVALID_CIPHERTEXT = "UZ-AUTH-019";
pub const ERR_INVALID_NONCE = "UZ-AUTH-020";
pub const ERR_INSUFFICIENT_SCOPE = "UZ-AUTH-022";
// API (serving-plane backpressure)
pub const ERR_API_BACKPRESSURE = "UZ-API-001";
pub const ERR_SSE_STREAM_CAP = "UZ-API-002";
// WORKSPACE
// BILLING
// SCORING
// ENTITLEMENT
// AGENT
pub const ERR_FLEET_KEY_NOT_FOUND = "UZ-FLEETKEY-001";
// PROFILE
// WEBHOOK
pub const ERR_WEBHOOK_NO_AGENT = "UZ-WH-001";
pub const ERR_WEBHOOK_MALFORMED = "UZ-WH-002";
// UZ-WH-003 retired (paused-ingress rework) — paused webhook ingress answers 200-ignored;
// steer uses ERR_AGENTSFLEET_PAUSED_INGRESS (UZ-AGT-012).
pub const ERR_WEBHOOK_SIG_INVALID = "UZ-WH-010";
pub const ERR_WEBHOOK_TIMESTAMP_STALE = "UZ-WH-011";
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = "UZ-WH-020";
pub const ERR_WEBHOOK_PAYLOAD_TOO_LARGE = "UZ-WH-030";
// TOOL
pub const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";
// AGENT
pub const ERR_AGENTSFLEET_CREDENTIAL_MISSING = "UZ-AGT-003";
pub const ERR_AGENTSFLEET_CLAIM_FAILED = "UZ-AGT-004";
pub const ERR_AGENTSFLEET_NAME_EXISTS = "UZ-AGT-006";
// UZ-AGT-007 retired — superseded by UZ-VAULT-002 (credential data too large).
pub const ERR_AGENTSFLEET_INVALID_CONFIG = "UZ-AGT-008";
pub const ERR_AGENTSFLEET_NOT_FOUND = "UZ-AGT-009";
pub const ERR_AGENTSFLEET_ALREADY_TERMINAL = "UZ-AGT-010";
pub const ERR_AGENTSFLEET_NAME_MISMATCH = "UZ-AGT-011";
pub const ERR_AGENTSFLEET_PAUSED_INGRESS = "UZ-AGT-012";
// Fleet Bundle
pub const ERR_FLEET_BUNDLE_INVALID = "UZ-BUNDLE-001";
pub const ERR_FLEET_BUNDLE_NOT_FOUND = "UZ-BUNDLE-002";
pub const ERR_FLEET_BUNDLE_CREDENTIALS_MISSING = "UZ-BUNDLE-003";
pub const ERR_FLEET_BUNDLE_FETCH_FAILED = "UZ-BUNDLE-004";
pub const ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE = "UZ-BUNDLE-005";
pub const ERR_FLEET_BUNDLE_TOO_MANY_IMPORTS = "UZ-BUNDLE-006";
// VAULT (structured-credential JSON shape)
pub const ERR_VAULT_DATA_INVALID = "UZ-VAULT-001";
pub const ERR_VAULT_DATA_TOO_LARGE = "UZ-VAULT-002";
pub const ERR_CREDENTIAL_NOT_FOUND = "UZ-VAULT-003";
// PROVIDER (tenant-scoped LLM provider config — PUT /v1/tenants/me/provider)
pub const ERR_PROVIDER_CREDENTIAL_REF_REQUIRED = "UZ-PROVIDER-001";
pub const ERR_PROVIDER_CREDENTIAL_NOT_FOUND = "UZ-PROVIDER-002";
pub const ERR_PROVIDER_CREDENTIAL_DATA_MALFORMED = "UZ-PROVIDER-003";
pub const ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE = "UZ-PROVIDER-004";
pub const ERR_PROVIDER_BASE_URL_INVALID = "UZ-PROVIDER-005";
pub const ERR_MODEL_CAP_NOT_FOUND = "UZ-PROVIDER-006";
pub const ERR_MODEL_CAP_IN_USE = "UZ-PROVIDER-007";
pub const ERR_MODEL_CAP_EXISTS = "UZ-PROVIDER-008";
// MEMORY
pub const ERR_MEM_AGENTSFLEET_NOT_FOUND = "UZ-MEM-002";
pub const ERR_MEM_UNAVAILABLE = "UZ-MEM-003";
// GATE
// STARTUP
pub const ERR_STARTUP_ENV_CHECK = "UZ-STARTUP-001";
pub const ERR_STARTUP_CONFIG_LOAD = "UZ-STARTUP-002";
pub const ERR_STARTUP_DB_CONNECT = "UZ-STARTUP-003";
pub const ERR_STARTUP_REDIS_CONNECT = "UZ-STARTUP-004";
pub const ERR_STARTUP_MIGRATION_CHECK = "UZ-STARTUP-005";
pub const ERR_STARTUP_ENV_ALLOC = "UZ-STARTUP-006";
// SANDBOX
// RUNNER
pub const ERR_EXEC_SESSION_CREATE_FAILED = "UZ-EXEC-001";
pub const ERR_EXEC_STAGE_START_FAILED = "UZ-EXEC-002";
pub const ERR_EXEC_TIMEOUT_KILL = "UZ-EXEC-003";
pub const ERR_EXEC_OOM_KILL = "UZ-EXEC-004";
pub const ERR_EXEC_RESOURCE_KILL = "UZ-EXEC-005";
pub const ERR_EXEC_TRANSPORT_LOSS = "UZ-EXEC-006";
pub const ERR_EXEC_LEASE_EXPIRED = "UZ-EXEC-007";
pub const ERR_EXEC_RENEWAL_TERMINATED = "UZ-EXEC-008";
pub const ERR_EXEC_STARTUP_POSTURE = "UZ-EXEC-009";
pub const ERR_EXEC_CRASH = "UZ-EXEC-010";
pub const ERR_EXEC_LANDLOCK_DENY = "UZ-EXEC-011";
pub const ERR_EXEC_RUNNER_FLEET_INIT = "UZ-EXEC-012";
pub const ERR_EXEC_RUNNER_FLEET_RUN = "UZ-EXEC-013";
pub const ERR_EXEC_RUNNER_INVALID_CONFIG = "UZ-EXEC-014";
// RELAY
// APPROVAL
pub const ERR_APPROVAL_PARSE_FAILED = "UZ-APPROVAL-001";
pub const ERR_APPROVAL_NOT_FOUND = "UZ-APPROVAL-002";
pub const ERR_APPROVAL_INVALID_SIGNATURE = "UZ-APPROVAL-003";
pub const ERR_APPROVAL_REDIS_UNAVAILABLE = "UZ-APPROVAL-004";
pub const ERR_APPROVAL_CONDITION_INVALID = "UZ-APPROVAL-005";
pub const ERR_APPROVAL_ALREADY_RESOLVED = "UZ-APPROVAL-006";
pub const ERR_APIKEY_INVALID = "UZ-APIKEY-001";
pub const ERR_APIKEY_NOT_FOUND = "UZ-APIKEY-003";
pub const ERR_APIKEY_REVOKED = "UZ-APIKEY-004";
pub const ERR_APIKEY_NAME_TAKEN = "UZ-APIKEY-005";
pub const ERR_APIKEY_ALREADY_REVOKED = "UZ-APIKEY-006";
pub const ERR_APIKEY_READONLY_FIELD = "UZ-APIKEY-007";
pub const ERR_APIKEY_MUST_REVOKE_FIRST = "UZ-APIKEY-008";
pub const ERR_GRANT_NOT_FOUND = "UZ-GRANT-001";
pub const ERR_GRANT_REVOKE_NOT_FOUND = "UZ-GRANT-002";
// RUNNER (agentsfleet-runner /v1/runners control contract)
pub const ERR_RUN_INVALID_RUNNER_TOKEN = "UZ-RUN-001";
pub const ERR_RUN_STALE_FENCING_TOKEN = "UZ-RUN-005";
pub const ERR_RUN_LEASE_NOT_FOUND = "UZ-RUN-006";
pub const ERR_RUN_SANDBOX_ESTABLISH_FAILED = "UZ-RUN-007";
pub const ERR_RUN_ADMIN_STATE_BLOCKED = "UZ-RUN-009";
pub const ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME = "UZ-RUN-010";
pub const ERR_RUN_LEASE_LOST = "UZ-RUN-011";
pub const ERR_RUN_LEASE_RENEWAL_NO_CREDITS = "UZ-RUN-012";
pub const ERR_RUN_RENEW_BODY_INVALID = "UZ-RUN-013";
pub const ERR_RUNNER_NOT_FOUND = "UZ-RUN-014";
// CREDENTIAL BROKER (M102 — on-demand mint via POST /v1/runners/me/credentials/mint)
pub const ERR_CRED_INTEGRATION_NOT_CONNECTED = "UZ-CRED-001";
pub const ERR_GH_RECONNECT_REQUIRED = "UZ-GH-001";
pub const ERR_GH_MINT_FAILED = "UZ-GH-002";
// GITHUB CONNECT (M102 §5 — the connect round-trip)
pub const ERR_CONNECTOR_NOT_CONFIGURED = "UZ-CONN-001";
pub const ERR_CONNECTOR_STATE_INVALID = "UZ-CONN-002";

// ── Non-error constants (migrated from codes.zig) ──────────────────────────
// Webhook user-facing messages
pub const MSG_BODY_REQUIRED = "Request body required";
pub const MSG_MALFORMED_JSON = "Malformed JSON";
pub const MSG_MISSING_FIELDS = "event_id and type are required";
pub const MSG_AGENTSFLEET_NOT_FOUND = "Fleet not found";
pub const MSG_AGENTSFLEET_NOT_ACTIVE = "Fleet is not active";
// Fleet CRUD messages
pub const MSG_AGENTSFLEET_NAME_EXISTS = "Fleet already exists in this workspace. Use `agentsfleet kill` first.";
pub const MSG_AGENTSFLEET_INVALID_CONFIG = "Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.";
pub const MSG_AGENTSFLEET_NAME_MISMATCH = "SKILL.md `name:` must match TRIGGER.md `name:`.";
pub const MSG_AGENTSFLEET_SKILL_INVALID = "SKILL.md frontmatter is invalid. Required: name (kebab, 1-64 chars), description, version (semver MAJOR.MINOR.PATCH).";
pub const MSG_AGENTSFLEET_NAME_REQUIRED = "name is required (max 64 chars, slug-safe)";
pub const MSG_AGENTSFLEET_SOURCE_REQUIRED = "source_markdown is required (max 64KB)";
pub const MSG_AGENTSFLEET_TRIGGER_REQUIRED = "trigger_markdown is required (max 64KB)";
pub const MSG_AGENTSFLEET_CONFIG_REQUIRED = "config_json is required";
pub const MSG_WORKSPACE_ID_REQUIRED = "workspace_id is required (UUIDv7)";
pub const MSG_CREDENTIAL_NAME_REQUIRED = "credential name is required (max 64 chars)";
pub const MSG_CREDENTIAL_DATA_REQUIRED = "credential data must be a non-empty JSON object";
pub const MSG_CREDENTIAL_DATA_TOO_LARGE = "credential data exceeds 4KB when stringified";
pub const MSG_CREDENTIAL_KEY_REQUIRED = "api_key is required and must be a non-empty string";
pub const MSG_CREDENTIAL_NOT_FOUND = "credential not found in this workspace";
// Serving-plane backpressure messages
pub const MSG_API_BACKPRESSURE = "Server is at its in-flight request ceiling";
pub const MSG_SSE_STREAM_CAP = "Concurrent event-stream limit reached on this instance";
// Approval messages
pub const MSG_APPROVAL_NOT_FOUND = "Approval action not found or already resolved";
pub const MSG_APPROVAL_INVALID_BODY = "Invalid approval payload";
pub const MSG_APPROVAL_INVALID_DECISION = "Decision must be 'approve' or 'deny'";
pub const MSG_APPROVAL_CONDITION_INVALID = "Gate condition is invalid. Use field == 'value' or field != 'value' (single-quoted).";
// Webhook signature messages
// Webhook constants
pub const BEARER_PREFIX = "Bearer ";
pub const DEDUP_TTL_SECONDS: u32 = 86400;
/// Redis key prefix for webhook idempotency slots (RULE UFS — one site; both
/// webhook handlers + tests import it).
pub const WEBHOOK_DEDUP_KEY_PREFIX = "webhook:dedup:";
pub const WEBHOOK_EVENT_TYPE = "webhook_received";
pub const STATUS_DUPLICATE = "duplicate";
/// Webhook 200-ignored reason for a paused/non-active fleet:
/// sender retry queues add no value for an intentionally paused fleet.
pub const IGNORED_REASON_AGENTSFLEET_PAUSED = "fleet_paused";
pub const STATUS_ACCEPTED = "accepted";
// Slack signature constants
pub const SLACK_SIG_VERSION = "v0";
pub const SLACK_SIG_HEADER = "x-slack-signature";
pub const SLACK_TS_HEADER = "x-slack-request-timestamp";
pub const SLACK_MAX_TS_DRIFT_SECONDS: i64 = 300;
// Gate constants
pub const GATE_DEFAULT_TIMEOUT_MS: u64 = 3_600_000;
/// Upper bound for a configured gate timeout — larger values clamp + warn.
pub const GATE_TIMEOUT_MS_MAX: u64 = 86_400_000;
pub const GATE_ANOMALY_KEY_PREFIX = "fleet:anomaly:";
pub const GATE_PENDING_KEY_PREFIX = "fleet:gate:pending:";
pub const GATE_RESPONSE_KEY_PREFIX = "fleet:gate:response:";
/// event_id → "action_id|deadline_ms" ref the async lease-path gate check reads.
pub const GATE_EVENT_REF_KEY_PREFIX = "fleet:gate:byevent:";
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
    @setEvalBranchQuota(EVAL_BRANCH_QUOTA);
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

// ── Comptime mirror-pin: auth_codes leaf must byte-match these codes ───────
// The auth plane imports these via the `auth_codes` named module (it cannot
// relative-import this file without breaking the test-auth portability gate).
// That leaf duplicates the literals; this pin makes any drift a compile error.
comptime {
    const auth_codes = @import("auth_codes");

    const pairs = .{
        .{ ERR_FORBIDDEN, auth_codes.ERR_FORBIDDEN },
        .{ ERR_UNAUTHORIZED, auth_codes.ERR_UNAUTHORIZED },
        .{ ERR_TOKEN_EXPIRED, auth_codes.ERR_TOKEN_EXPIRED },
        .{ ERR_AUTH_UNAVAILABLE, auth_codes.ERR_AUTH_UNAVAILABLE },
        .{ ERR_INSUFFICIENT_SCOPE, auth_codes.ERR_INSUFFICIENT_SCOPE },
        .{ ERR_APPROVAL_INVALID_SIGNATURE, auth_codes.ERR_APPROVAL_INVALID_SIGNATURE },
        .{ ERR_WEBHOOK_SIG_INVALID, auth_codes.ERR_WEBHOOK_SIG_INVALID },
        .{ ERR_WEBHOOK_TIMESTAMP_STALE, auth_codes.ERR_WEBHOOK_TIMESTAMP_STALE },
        .{ ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, auth_codes.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED },
        .{ ERR_APIKEY_REVOKED, auth_codes.ERR_APIKEY_REVOKED },
        .{ ERR_RUN_INVALID_RUNNER_TOKEN, auth_codes.ERR_RUN_INVALID_RUNNER_TOKEN },
        .{ ERR_RUN_ADMIN_STATE_BLOCKED, auth_codes.ERR_RUN_ADMIN_STATE_BLOCKED },
        .{ ERR_INTERNAL_OPERATION_FAILED, auth_codes.ERR_INTERNAL_OPERATION_FAILED },
    };
    for (pairs) |p| {
        if (!std.mem.eql(u8, p[0], p[1]))
            @compileError("auth_codes mirror drift: " ++ p[0]);
    }
}

test {
    _ = @import("codes_test.zig");
    _ = @import("error_registry_test.zig");
}
