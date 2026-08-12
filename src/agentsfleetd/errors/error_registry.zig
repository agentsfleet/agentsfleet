/// The error-code declaration surface: every ERR_* constant and its message.
///
/// Add codes to error_entries.zig or error_entries_runtime.zig, then define the
/// matching ERR_* constant below; the comptime self-check at the bottom rejects
/// a constant with no entry.
///
/// Code literals may live ONLY in this file. `audits/error-codes.sh` greps this
/// path alone for declared codes, so a code declared in a sibling reads as an
/// orphan at every use site. Anything that is not a code declaration belongs
/// elsewhere — the lookup machinery is in error_lookup.zig (re-exported below),
/// and the approval-gate and webhook vocabularies moved to the families that
/// speak them.
const std = @import("std");
const error_lookup = @import("error_lookup.zig");
const EVAL_BRANCH_QUOTA = 1_000_000;

pub const Entry = error_lookup.Entry;
pub const UNKNOWN = error_lookup.UNKNOWN;
pub const ERROR_DOCS_BASE = error_lookup.ERROR_DOCS_BASE;
pub const REGISTRY = error_lookup.REGISTRY;
pub const lookup = error_lookup.lookup;
pub const hint = error_lookup.hint;

// UUIDV7
pub const ERR_UUIDV7_INVALID_ID_SHAPE = "UZ-UUIDV7-009";
// INTERNAL
pub const ERR_INTERNAL_DB_UNAVAILABLE = "UZ-INTERNAL-001";
pub const ERR_INTERNAL_DB_QUERY = "UZ-INTERNAL-002";
pub const ERR_INTERNAL_OPERATION_FAILED = "UZ-INTERNAL-003";
// REQUEST
pub const ERR_INVALID_REQUEST = "UZ-REQ-001";
pub const ERR_PAYLOAD_TOO_LARGE = "UZ-REQ-002";
// WORKSPACE
pub const ERR_WORKSPACE_NAME_EXISTS = "UZ-WORKSPACE-001";
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
pub const ERR_CLI_CREDENTIAL_REVOKED = "UZ-AUTH-023";
pub const ERR_CLI_CREDENTIAL_NOT_FOUND = "UZ-AUTH-024";
// API (serving-plane backpressure)
pub const ERR_API_BACKPRESSURE = "UZ-API-001";
pub const ERR_SSE_STREAM_CAP = "UZ-API-002";
// AGENT
// WEBHOOK
pub const ERR_WEBHOOK_NO_AGENT = "UZ-WH-001";
pub const ERR_WEBHOOK_MALFORMED = "UZ-WH-002";
// UZ-WH-003 retired (paused-ingress rework) — paused webhook ingress answers 200-ignored;
// steer uses ERR_AGENTSFLEET_PAUSED_INGRESS (UZ-AGT-012).
pub const ERR_WEBHOOK_SIG_INVALID = "UZ-WH-010";
pub const ERR_WEBHOOK_TIMESTAMP_STALE = "UZ-WH-011";
pub const ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED = "UZ-WH-020";
pub const ERR_WEBHOOK_INSTALL_NOT_MAPPED = "UZ-WH-021";
pub const ERR_WEBHOOK_SUBSCRIPTION_NOT_FOUND = "UZ-WH-022";
pub const ERR_WEBHOOK_PAYLOAD_TOO_LARGE = "UZ-WH-030";
// SLACK CONNECTOR
// Events ingress (M106 §2): signature/replay rejections are 401; an unmapped
// team is a 200-ack no-op (Slack must never see an error loop) — UZ-SLK-020 is
// its structured log/telemetry reason, not a wire status.
pub const ERR_SLACK_SIG_INVALID = "UZ-SLK-010";
pub const ERR_SLACK_TIMESTAMP_STALE = "UZ-SLK-011";
pub const ERR_SLACK_TEAM_NOT_INSTALLED = "UZ-SLK-020";
pub const ERR_SLACK_OAUTH_EXCHANGE_FAILED = "UZ-SLK-022";
// Outbound answer post (§4): a log-only code — the connector:outbound worker
// logs a failed chat.postMessage and retries with backoff; it is never an
// `hx.fail` wire status (no HTTP caller waits on the answer). Carries a
// `.bad_gateway` status for symmetry with UZ-SLK-022 (both Slack-upstream).
pub const ERR_SLACK_OUTBOUND_POST_FAILED = "UZ-SLK-030";
// TOOL
pub const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";
// AGENT
pub const ERR_AGENTSFLEET_CREDENTIAL_MISSING = "UZ-AGT-003";
pub const ERR_AGENTSFLEET_CLAIM_FAILED = "UZ-AGT-004";
pub const ERR_AGENTSFLEET_NAME_EXISTS = "UZ-AGT-006";
pub const ERR_AGENTSFLEET_INVALID_CONFIG = "UZ-AGT-008"; // UZ-AGT-007 retired — superseded by UZ-VAULT-002.
pub const ERR_AGENTSFLEET_NOT_FOUND = "UZ-AGT-009";
pub const ERR_AGENTSFLEET_ALREADY_TERMINAL = "UZ-AGT-010";
pub const ERR_AGENTSFLEET_NAME_MISMATCH = "UZ-AGT-011";
pub const ERR_AGENTSFLEET_PAUSED_INGRESS = "UZ-AGT-012";
pub const ERR_AGENTSFLEET_INSTALL_ROLLED_BACK = "UZ-AGT-013";
pub const ERR_AGENTSFLEET_SOURCE_STALE = "UZ-AGT-014";
pub const ERR_EVENT_NOT_FOUND = "UZ-AGT-015";
// SCHEDULE
pub const ERR_SCHEDULE_INVALID = "UZ-SCHED-001";
pub const ERR_SCHEDULE_NOT_FOUND = "UZ-SCHED-002";
pub const ERR_SCHEDULE_LIMIT_REACHED = "UZ-SCHED-003";
pub const ERR_SCHEDULE_PROVIDER_UNAVAILABLE = "UZ-SCHED-004";
pub const ERR_SCHEDULE_SIGNATURE_INVALID = "UZ-SCHED-005";
pub const ERR_SCHEDULE_UPDATE_BUSY = "UZ-SCHED-006";
pub const ERR_SCHEDULE_NOT_CONFIGURED = "UZ-SCHED-007";
pub const ERR_SCHEDULE_CONFLICT = "UZ-SCHED-008";
// Fleet Bundle
pub const ERR_FLEET_BUNDLE_INVALID = "UZ-BUNDLE-001";
pub const ERR_FLEET_BUNDLE_NOT_FOUND = "UZ-BUNDLE-002";
pub const ERR_FLEET_BUNDLE_SECRETS_MISSING = "UZ-BUNDLE-003";
pub const ERR_FLEET_BUNDLE_FETCH_FAILED = "UZ-BUNDLE-004";
pub const ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE = "UZ-BUNDLE-005";
// UZ-BUNDLE-006 retired — no producer ever emitted it.
// CATALOG (platform fleet-library lifecycle — /v1/admin/fleet-libraries)
pub const ERR_CATALOG_NOT_FOUND = "UZ-CATALOG-001";
pub const ERR_CATALOG_PUBLISH_WITHOUT_BUNDLE = "UZ-CATALOG-002";
pub const ERR_CATALOG_DELETE_PUBLISHED = "UZ-CATALOG-003";
pub const ERR_CATALOG_ID_COLLISION = "UZ-CATALOG-004";
pub const ERR_CATALOG_ROW_STALE = "UZ-CATALOG-005";
// VAULT (structured-credential JSON shape)
pub const ERR_VAULT_DATA_INVALID = "UZ-VAULT-001";
pub const ERR_VAULT_DATA_TOO_LARGE = "UZ-VAULT-002";
pub const ERR_SECRET_NOT_FOUND = "UZ-VAULT-003";
pub const ERR_SECRET_REFERENCED_BY_MODEL_ENTRIES = "UZ-VAULT-004";
pub const ERR_SECRET_NAME_TAKEN = "UZ-VAULT-005";
// PROVIDER (tenant-scoped LLM provider config — PUT /v1/tenants/me/provider)
pub const ERR_PROVIDER_SECRET_REF_REQUIRED = "UZ-PROVIDER-001";
pub const ERR_PROVIDER_SECRET_NOT_FOUND = "UZ-PROVIDER-002";
pub const ERR_PROVIDER_SECRET_DATA_MALFORMED = "UZ-PROVIDER-003";
pub const ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE = "UZ-PROVIDER-004";
pub const ERR_PROVIDER_BASE_URL_INVALID = "UZ-PROVIDER-005";
pub const ERR_MODEL_CAP_NOT_FOUND = "UZ-PROVIDER-006";
pub const ERR_MODEL_CAP_IN_USE = "UZ-PROVIDER-007";
pub const ERR_MODEL_CAP_EXISTS = "UZ-PROVIDER-008";
pub const ERR_PROVIDER_PLATFORM_KEY_MISSING = "UZ-PROVIDER-009";
pub const ERR_TENANT_NO_PRIMARY_WORKSPACE = "UZ-PROVIDER-010";
// MODELS (tenant model registry — /v1/tenants/me/models)
pub const ERR_MODELS_DELETE_ACTIVE = "UZ-MODELS-001";
pub const ERR_MODELS_SECRET_NOT_FOUND = "UZ-MODELS-002";
pub const ERR_MODELS_DUPLICATE_ENTRY = "UZ-MODELS-003";
pub const ERR_MODELS_ENTRY_NOT_FOUND = "UZ-MODELS-004";
// LIBRARY (bounded library reads — tenant models, catalogue, Fleet gallery)
pub const ERR_LIBRARY_CURSOR_MALFORMED = "UZ-LIBRARY-001";
pub const ERR_LIBRARY_CURSOR_MISMATCH = "UZ-LIBRARY-002";
pub const ERR_LIBRARY_INPUT_OUT_OF_BOUNDS = "UZ-LIBRARY-003";
pub const ERR_LIBRARY_REVISION_UNAVAILABLE = "UZ-LIBRARY-004";
pub const ERR_LIBRARY_BODY_CEILING = "UZ-LIBRARY-005";
pub const ERR_LIBRARY_DB_UNAVAILABLE = "UZ-LIBRARY-006";
pub const ERR_LIBRARY_REFERENCE_RACE = "UZ-LIBRARY-008";
// PREFS (per-user dashboard UI prefs — /v1/workspaces/{workspace_id}/preferences)
pub const ERR_PREF_KEY_UNKNOWN = "UZ-PREFS-001";
pub const ERR_PREF_VALUE_TOO_LARGE = "UZ-PREFS-002";
// MEMORY
pub const ERR_MEM_AGENTSFLEET_NOT_FOUND = "UZ-MEM-002";
pub const ERR_MEM_UNAVAILABLE = "UZ-MEM-003";
pub const ERR_MEM_ENTRY_NOT_FOUND = "UZ-MEM-004";
// GATE
// STARTUP
pub const ERR_STARTUP_ENV_CHECK = "UZ-STARTUP-001";
pub const ERR_STARTUP_CONFIG_LOAD = "UZ-STARTUP-002";
pub const ERR_STARTUP_DB_CONNECT = "UZ-STARTUP-003";
pub const ERR_STARTUP_REDIS_CONNECT = "UZ-STARTUP-004";
pub const ERR_STARTUP_MIGRATION_CHECK = "UZ-STARTUP-005";
pub const ERR_STARTUP_ENV_ALLOC = "UZ-STARTUP-006";
// RUNNER
pub const ERR_EXEC_TIMEOUT_KILL = "UZ-EXEC-003"; // UZ-EXEC-001 retired: no producer. UZ-EXEC-002 retired: no producer.
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
pub const ERR_EXEC_BUDGET_BREACH = "UZ-EXEC-015";
pub const ERR_EXEC_RUNNER_TOKEN_REJECTED = "UZ-EXEC-016";
pub const ERR_EXEC_ASSIGNMENT_UNACHIEVABLE = "UZ-EXEC-017";
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
pub const ERR_GRANT_ALREADY_RESOLVED = "UZ-GRANT-003";
// REPAIR (write-scoped mint behind the repository-write approval)
pub const ERR_REPAIR_WRITE_UNAPPROVED = "UZ-REPAIR-010";
pub const ERR_REPAIR_BINDING_DRIFT = "UZ-REPAIR-011";
pub const ERR_REPAIR_DUPLICATE_LINK = "UZ-REPAIR-012";
// RUNNER (agentsfleet-runner /v1/runners control contract)
pub const ERR_RUN_INVALID_RUNNER_TOKEN = "UZ-RUN-001";
pub const ERR_RUN_STALE_FENCING_TOKEN = "UZ-RUN-005";
pub const ERR_RUN_LEASE_NOT_FOUND = "UZ-RUN-006";
// UZ-RUN-007 retired — no agentsfleetd producer ever emitted it.
pub const ERR_RUN_ADMIN_STATE_BLOCKED = "UZ-RUN-009";
pub const ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME = "UZ-RUN-010";
pub const ERR_RUN_LEASE_LOST = "UZ-RUN-011";
pub const ERR_RUN_LEASE_RENEWAL_NO_CREDITS = "UZ-RUN-012";
pub const ERR_RUN_RENEW_BODY_INVALID = "UZ-RUN-013";
pub const ERR_RUNNER_NOT_FOUND = "UZ-RUN-014";
/// The FLEET's own spend ceiling, not the tenant's credit pool (UZ-RUN-012).
pub const ERR_RUN_BUDGET_EXCEEDED = "UZ-RUN-015";
pub const ERR_RUNNER_MUST_REVOKE_FIRST = "UZ-RUN-016"; // mirrors ERR_APIKEY_MUST_REVOKE_FIRST
// CREDENTIAL BROKER (M102 — on-demand mint via POST /v1/runners/me/credentials/mint)
pub const ERR_CRED_INTEGRATION_NOT_CONNECTED = "UZ-CRED-001";
pub const ERR_CRED_BROKER_NOT_CONFIGURED = "UZ-CRED-002";
pub const ERR_GH_RECONNECT_REQUIRED = "UZ-GH-001";
pub const ERR_GH_MINT_FAILED = "UZ-GH-002";
// CONNECTOR PLATFORM (the connect round-trip + bounded vendor calls — any provider)
pub const ERR_CONNECTOR_NOT_CONFIGURED = "UZ-CONN-001";
pub const ERR_CONNECTOR_STATE_INVALID = "UZ-CONN-002";
pub const ERR_CONNECTOR_VENDOR_DEADLINE = "UZ-CONN-003";
pub const ERR_CONNECTOR_UNKNOWN = "UZ-CONN-004";
// UZ-CONN-005 (connector probe rejected) retired — the api_key connect probe was
// removed when api-key providers became custom secrets rather than connectors.
pub const ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED = "UZ-CONN-006";
pub const ERR_CONNECTOR_CATALOG_LOOKUP_FAILED = "UZ-CONN-007";
pub const ERR_CONNECTOR_INSTALLATION_OWNERSHIP = "UZ-CONN-008";

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
pub const MSG_AGENTSFLEET_SOURCE_STALE = "The fleet source changed since you read it; refetch and reapply your edit";
pub const MSG_WORKSPACE_ID_REQUIRED = "workspace_id is required (UUIDv7)";
pub const MSG_SECRET_NAME_REQUIRED = "secret name is required (max 64 chars)";
pub const MSG_SECRET_DATA_REQUIRED = "secret data must be a non-empty JSON object";
pub const MSG_SECRET_DATA_TOO_LARGE = "secret data exceeds 4KB when stringified";
pub const MSG_SECRET_NOT_FOUND = "secret not found in this workspace";
pub const MSG_SECRET_NAME_TAKEN = "a secret with this name already exists in this workspace; replace its value with `secret update` instead of creating it again";
// Serving-plane backpressure messages
pub const MSG_API_BACKPRESSURE = "Server is at its in-flight request ceiling";
pub const MSG_SSE_STREAM_CAP = "Concurrent event-stream limit reached on this instance";
// Approval messages
pub const MSG_APPROVAL_NOT_FOUND = "Approval action not found or already resolved";
pub const MSG_APPROVAL_INVALID_BODY = "Invalid approval payload";
pub const MSG_APPROVAL_INVALID_DECISION = "Decision must be 'approve' or 'deny'";
pub const MSG_APPROVAL_CONDITION_INVALID = "Gate condition is invalid. Use field == 'value' or field != 'value' (single-quoted).";
// ── Comptime self-check: every ERR_* constant exists in REGISTRY ───────────
comptime {
    @setEvalBranchQuota(EVAL_BRANCH_QUOTA);
    const decls = @typeInfo(@This()).@"struct".decls;
    for (decls) |decl| {
        if (std.mem.startsWith(u8, decl.name, "ERR_")) {
            const code: []const u8 = @field(@This(), decl.name);
            if (!error_lookup.isRegistered(code)) {
                @compileError("ERR_* constant not in REGISTRY: " ++ code);
            }
        }
    }
}

// The split mirror pin still runs eagerly once this import forces analysis.
comptime {
    _ = @import("error_registry_mirror_pin.zig");
}

// Not imported by gen_error_codes.zig's narrow exe module (deliberately —
// that tool imports error_entries*.zig directly to avoid this test graph).
test {
    _ = @import("codes_test.zig");
    _ = @import("error_registry_test.zig");
    _ = @import("error_registry_promoted_test.zig");
    _ = @import("internal_op_error_sweep_test.zig");
    _ = @import("gen_error_codes_test.zig");
    _ = @import("mudball_guard_test.zig");
    _ = @import("error_entries_reachability_test.zig");
    _ = @import("error_registry_hygiene_test.zig");
    _ = @import("error_registry_reachability_fix_test.zig");
}
