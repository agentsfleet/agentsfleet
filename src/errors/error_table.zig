/// error_table.zig — code → (http_status, title, docs_uri) registry.
///
/// Every error code declared in codes.zig must have an entry here.
/// lookup(code) returns the entry; callers no longer pass std.http.Status inline.
/// UNKNOWN_ENTRY is the fallback for unregistered codes — always 500.
///
/// Adding a new error code: add one entry to TABLE. The test at the bottom
/// verifies spot-coverage; the comptime block guards exhaustive coverage.
const std = @import("std");

pub const ERROR_DOCS_BASE = "https://docs.usezombie.com/error-codes#";

pub const ErrorEntry = struct {
    code: []const u8,
    http_status: std.http.Status,
    title: []const u8,
    docs_uri: []const u8,
};

/// Sentinel returned when lookup() finds no matching TABLE entry.
/// .code is a distinct sentinel — it is never in TABLE.
/// errorResponse() uses the caller-supplied code in the response body,
/// not UNKNOWN_ENTRY.code, so the sentinel never leaks into wire output.
pub const UNKNOWN_ENTRY = ErrorEntry{
    .code = "UZ-UNKNOWN",
    .http_status = .internal_server_error,
    .title = "Unregistered error code",
    .docs_uri = ERROR_DOCS_BASE ++ "UZ-INTERNAL-003",
};

pub const TABLE = [_]ErrorEntry{
    // ── UUIDV7 ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-UUIDV7-003", .http_status = .bad_request,           .title = "Invalid UUID canonical format",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-003" },
    .{ .code = "UZ-UUIDV7-005", .http_status = .internal_server_error, .title = "ID generation failed",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-005" },
    .{ .code = "UZ-UUIDV7-009", .http_status = .bad_request,           .title = "Invalid ID shape",                 .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-009" },
    .{ .code = "UZ-UUIDV7-010", .http_status = .conflict,              .title = "UUID backfill conflict",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-010" },
    .{ .code = "UZ-UUIDV7-011", .http_status = .internal_server_error, .title = "Rollback blocked",                 .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-011" },
    .{ .code = "UZ-UUIDV7-012", .http_status = .internal_server_error, .title = "Error response linking failed",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-UUIDV7-012" },
    // ── INTERNAL ─────────────────────────────────────────────────────────────
    .{ .code = "UZ-INTERNAL-001", .http_status = .service_unavailable,  .title = "Database unavailable",            .docs_uri = ERROR_DOCS_BASE ++ "UZ-INTERNAL-001" },
    .{ .code = "UZ-INTERNAL-002", .http_status = .internal_server_error,.title = "Database error",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-INTERNAL-002" },
    .{ .code = "UZ-INTERNAL-003", .http_status = .internal_server_error,.title = "Internal operation failed",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-INTERNAL-003" },
    // ── REQUEST ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-REQ-001", .http_status = .bad_request,       .title = "Invalid request",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-REQ-001" },
    .{ .code = "UZ-REQ-002", .http_status = .payload_too_large, .title = "Payload too large",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-REQ-002" },
    // ── AUTH ─────────────────────────────────────────────────────────────────
    .{ .code = "UZ-AUTH-001", .http_status = .forbidden,           .title = "Forbidden",                          .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-001" },
    .{ .code = "UZ-AUTH-002", .http_status = .unauthorized,        .title = "Unauthorized",                       .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-002" },
    .{ .code = "UZ-AUTH-003", .http_status = .unauthorized,        .title = "Token expired",                      .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-003" },
    .{ .code = "UZ-AUTH-004", .http_status = .service_unavailable, .title = "Authentication service unavailable", .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-004" },
    .{ .code = "UZ-AUTH-005", .http_status = .not_found,           .title = "Session not found",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-005" },
    .{ .code = "UZ-AUTH-006", .http_status = .unauthorized,        .title = "Session expired",                    .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-006" },
    .{ .code = "UZ-AUTH-007", .http_status = .conflict,            .title = "Session already complete",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-007" },
    .{ .code = "UZ-AUTH-008", .http_status = .service_unavailable, .title = "Session limit reached",              .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-008" },
    .{ .code = "UZ-AUTH-009", .http_status = .forbidden,           .title = "Insufficient role",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-009" },
    .{ .code = "UZ-AUTH-010", .http_status = .forbidden,           .title = "Unsupported role",                   .docs_uri = ERROR_DOCS_BASE ++ "UZ-AUTH-010" },
    // ── API / QUEUE ──────────────────────────────────────────────────────────
    .{ .code = "UZ-API-001", .http_status = .service_unavailable, .title = "API saturated",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-API-001" },
    .{ .code = "UZ-API-002", .http_status = .service_unavailable, .title = "Queue unavailable",   .docs_uri = ERROR_DOCS_BASE ++ "UZ-API-002" },
    // ── WORKSPACE ────────────────────────────────────────────────────────────
    .{ .code = "UZ-WORKSPACE-001", .http_status = .not_found,        .title = "Workspace not found",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-WORKSPACE-001" },
    .{ .code = "UZ-WORKSPACE-002", .http_status = .payment_required, .title = "Workspace paused",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-WORKSPACE-002" },
    .{ .code = "UZ-WORKSPACE-003", .http_status = .payment_required, .title = "Workspace free limit reached",.docs_uri = ERROR_DOCS_BASE ++ "UZ-WORKSPACE-003" },
    // ── BILLING ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-BILLING-001", .http_status = .bad_request,          .title = "Invalid subscription ID",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-BILLING-001" },
    .{ .code = "UZ-BILLING-002", .http_status = .internal_server_error,.title = "Billing state missing",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-BILLING-002" },
    .{ .code = "UZ-BILLING-003", .http_status = .internal_server_error,.title = "Billing state invalid",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-BILLING-003" },
    .{ .code = "UZ-BILLING-004", .http_status = .bad_request,          .title = "Invalid billing event",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-BILLING-004" },
    .{ .code = "UZ-BILLING-005", .http_status = .payment_required,     .title = "Credit exhausted",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-BILLING-005" },
    // ── SCORING ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-SCORING-001", .http_status = .bad_request, .title = "Invalid scoring context", .docs_uri = ERROR_DOCS_BASE ++ "UZ-SCORING-001" },
    // ── ENTITLEMENT ──────────────────────────────────────────────────────────
    .{ .code = "UZ-ENTL-001", .http_status = .service_unavailable, .title = "Entitlement service unavailable", .docs_uri = ERROR_DOCS_BASE ++ "UZ-ENTL-001" },
    .{ .code = "UZ-ENTL-002", .http_status = .payment_required,    .title = "Profile limit reached",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-ENTL-002" },
    .{ .code = "UZ-ENTL-003", .http_status = .payment_required,    .title = "Stage limit reached",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-ENTL-003" },
    .{ .code = "UZ-ENTL-004", .http_status = .forbidden,           .title = "Skill not allowed",               .docs_uri = ERROR_DOCS_BASE ++ "UZ-ENTL-004" },
    // ── SPEC ─────────────────────────────────────────────────────────────────
    .{ .code = "UZ-SPEC-001", .http_status = .not_found,           .title = "Spec not found",                .docs_uri = ERROR_DOCS_BASE ++ "UZ-SPEC-001" },
    .{ .code = "UZ-SPEC-002", .http_status = .bad_request,         .title = "Spec is empty",                 .docs_uri = ERROR_DOCS_BASE ++ "UZ-SPEC-002" },
    .{ .code = "UZ-SPEC-003", .http_status = .unprocessable_entity,.title = "Spec has no actionable content",.docs_uri = ERROR_DOCS_BASE ++ "UZ-SPEC-003" },
    .{ .code = "UZ-SPEC-004", .http_status = .unprocessable_entity,.title = "Spec has unresolved file ref",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-SPEC-004" },
    // ── RUN ──────────────────────────────────────────────────────────────────
    .{ .code = "UZ-RUN-001", .http_status = .not_found,           .title = "Run not found",                      .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-001" },
    .{ .code = "UZ-RUN-002", .http_status = .conflict,            .title = "Invalid state transition",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-002" },
    .{ .code = "UZ-RUN-003", .http_status = .too_many_requests,   .title = "Run token budget exceeded",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-003" },
    .{ .code = "UZ-RUN-004", .http_status = .request_timeout,     .title = "Run wall time exceeded",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-004" },
    .{ .code = "UZ-RUN-005", .http_status = .too_many_requests,   .title = "Workspace monthly budget exceeded",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-005" },
    .{ .code = "UZ-RUN-006", .http_status = .conflict,            .title = "Run already in terminal state",      .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-006" },
    .{ .code = "UZ-RUN-007", .http_status = .internal_server_error,.title = "Run cancel signal failed",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-007" },
    .{ .code = "UZ-RUN-008", .http_status = .internal_server_error,.title = "Run interrupt signal failed",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-008" },
    .{ .code = "UZ-RUN-009", .http_status = .conflict,            .title = "Run not interruptible",              .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUN-009" },
    // ── AGENT ────────────────────────────────────────────────────────────────
    .{ .code = "UZ-AGENT-001", .http_status = .not_found,          .title = "Agent not found",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-AGENT-001" },
    .{ .code = "UZ-AGENT-002", .http_status = .service_unavailable,.title = "Agent scores unavailable",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-AGENT-002" },
    // ── PROFILE ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-PROFILE-001", .http_status = .not_found,   .title = "Profile not found", .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROFILE-001" },
    .{ .code = "UZ-PROFILE-002", .http_status = .bad_request, .title = "Invalid profile",   .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROFILE-002" },
    // ── PROPOSAL ─────────────────────────────────────────────────────────────
    .{ .code = "UZ-PROPOSAL-001", .http_status = .bad_request,          .title = "Invalid proposal JSON",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-001" },
    .{ .code = "UZ-PROPOSAL-002", .http_status = .bad_request,          .title = "Proposal not an array",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-002" },
    .{ .code = "UZ-PROPOSAL-003", .http_status = .bad_request,          .title = "Proposal change not an object",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-003" },
    .{ .code = "UZ-PROPOSAL-004", .http_status = .bad_request,          .title = "Missing target field",              .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-004" },
    .{ .code = "UZ-PROPOSAL-005", .http_status = .bad_request,          .title = "Unsupported target field",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-005" },
    .{ .code = "UZ-PROPOSAL-006", .http_status = .bad_request,          .title = "Missing stage ID",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-006" },
    .{ .code = "UZ-PROPOSAL-007", .http_status = .bad_request,          .title = "Missing role",                      .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-007" },
    .{ .code = "UZ-PROPOSAL-008", .http_status = .bad_request,          .title = "Missing insert-before stage ID",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-008" },
    .{ .code = "UZ-PROPOSAL-009", .http_status = .bad_request,          .title = "Disallowed field",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-009" },
    .{ .code = "UZ-PROPOSAL-010", .http_status = .bad_request,          .title = "Unregistered agent reference",      .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-010" },
    .{ .code = "UZ-PROPOSAL-011", .http_status = .bad_request,          .title = "Invalid skill reference",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-011" },
    .{ .code = "UZ-PROPOSAL-012", .http_status = .bad_request,          .title = "Unknown stage reference",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-012" },
    .{ .code = "UZ-PROPOSAL-013", .http_status = .conflict,             .title = "Duplicate stage reference",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-013" },
    .{ .code = "UZ-PROPOSAL-014", .http_status = .unprocessable_entity, .title = "Proposal would not compile",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-014" },
    .{ .code = "UZ-PROPOSAL-015", .http_status = .unprocessable_entity, .title = "No valid proposal template",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-015" },
    .{ .code = "UZ-PROPOSAL-016", .http_status = .internal_server_error,.title = "Proposal generation failed",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-016" },
    .{ .code = "UZ-PROPOSAL-017", .http_status = .not_found,            .title = "Proposal not found",                .docs_uri = ERROR_DOCS_BASE ++ "UZ-PROPOSAL-017" },
    // ── HARNESS ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-HARNESS-001", .http_status = .not_found, .title = "Harness change not found", .docs_uri = ERROR_DOCS_BASE ++ "UZ-HARNESS-001" },
    // ── WEBHOOK ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-WH-001", .http_status = .not_found,   .title = "Zombie not found for webhook",   .docs_uri = ERROR_DOCS_BASE ++ "UZ-WH-001" },
    .{ .code = "UZ-WH-002", .http_status = .bad_request, .title = "Malformed webhook",              .docs_uri = ERROR_DOCS_BASE ++ "UZ-WH-002" },
    .{ .code = "UZ-WH-003", .http_status = .conflict,    .title = "Zombie paused",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-WH-003" },
    .{ .code = "UZ-WH-010", .http_status = .unauthorized,.title = "Invalid webhook signature",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-WH-010" },
    .{ .code = "UZ-WH-011", .http_status = .unauthorized,.title = "Stale webhook timestamp",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-WH-011" },
    // ── TOOL ─────────────────────────────────────────────────────────────────
    .{ .code = "UZ-TOOL-001", .http_status = .failed_dependency,.title = "Tool credential missing",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-001" },
    .{ .code = "UZ-TOOL-002", .http_status = .bad_gateway,      .title = "Tool API call failed",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-002" },
    .{ .code = "UZ-TOOL-003", .http_status = .bad_gateway,      .title = "Tool git operation failed",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-003" },
    .{ .code = "UZ-TOOL-004", .http_status = .bad_request,      .title = "Tool not attached",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-004" },
    .{ .code = "UZ-TOOL-005", .http_status = .bad_request,      .title = "Unknown tool",               .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-005" },
    .{ .code = "UZ-TOOL-006", .http_status = .gateway_timeout,  .title = "Tool call timed out",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-TOOL-006" },
    // ── ZOMBIE ───────────────────────────────────────────────────────────────
    .{ .code = "UZ-ZMB-001", .http_status = .payment_required,     .title = "Zombie budget exceeded",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-001" },
    .{ .code = "UZ-ZMB-002", .http_status = .internal_server_error,.title = "Zombie agent timeout",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-002" },
    .{ .code = "UZ-ZMB-003", .http_status = .failed_dependency,    .title = "Zombie credential missing",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-003" },
    .{ .code = "UZ-ZMB-004", .http_status = .internal_server_error,.title = "Zombie claim failed",              .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-004" },
    .{ .code = "UZ-ZMB-005", .http_status = .internal_server_error,.title = "Zombie checkpoint failed",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-005" },
    .{ .code = "UZ-ZMB-006", .http_status = .conflict,             .title = "Zombie name already exists",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-006" },
    .{ .code = "UZ-ZMB-007", .http_status = .bad_request,          .title = "Zombie credential value too long", .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-007" },
    .{ .code = "UZ-ZMB-008", .http_status = .bad_request,          .title = "Invalid zombie config",            .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-008" },
    .{ .code = "UZ-ZMB-009", .http_status = .not_found,            .title = "Zombie not found",                 .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-009" },
    // ── GATE ─────────────────────────────────────────────────────────────────
    .{ .code = "UZ-GATE-001", .http_status = .internal_server_error,.title = "Gate command failed",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-GATE-001" },
    .{ .code = "UZ-GATE-002", .http_status = .gateway_timeout,      .title = "Gate command timed out",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-GATE-002" },
    .{ .code = "UZ-GATE-003", .http_status = .internal_server_error,.title = "Gate repair attempts exhausted",.docs_uri = ERROR_DOCS_BASE ++ "UZ-GATE-003" },
    .{ .code = "UZ-GATE-004", .http_status = .internal_server_error,.title = "Gate persist failed",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-GATE-004" },
    // ── STARTUP ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-STARTUP-001", .http_status = .internal_server_error,.title = "Environment check failed",   .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-001" },
    .{ .code = "UZ-STARTUP-002", .http_status = .internal_server_error,.title = "Config load failed",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-002" },
    .{ .code = "UZ-STARTUP-003", .http_status = .internal_server_error,.title = "Database connect failed",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-003" },
    .{ .code = "UZ-STARTUP-004", .http_status = .internal_server_error,.title = "Redis connect failed",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-004" },
    .{ .code = "UZ-STARTUP-005", .http_status = .internal_server_error,.title = "Migration check failed",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-005" },
    .{ .code = "UZ-STARTUP-006", .http_status = .internal_server_error,.title = "OIDC init failed",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-006" },
    .{ .code = "UZ-STARTUP-007", .http_status = .internal_server_error,.title = "Redis group creation failed",.docs_uri = ERROR_DOCS_BASE ++ "UZ-STARTUP-007" },
    // ── SANDBOX ──────────────────────────────────────────────────────────────
    .{ .code = "UZ-SANDBOX-001", .http_status = .service_unavailable,.title = "Sandbox backend unavailable",      .docs_uri = ERROR_DOCS_BASE ++ "UZ-SANDBOX-001" },
    .{ .code = "UZ-SANDBOX-002", .http_status = .forbidden,          .title = "Sandbox kill switch triggered",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-SANDBOX-002" },
    .{ .code = "UZ-SANDBOX-003", .http_status = .forbidden,          .title = "Sandbox command blocked",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-SANDBOX-003" },
    // ── WORKER ───────────────────────────────────────────────────────────────
    .{ .code = "UZ-WORKER-001", .http_status = .internal_server_error,.title = "Worker prompts load failed",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-WORKER-001" },
    .{ .code = "UZ-WORKER-002", .http_status = .internal_server_error,.title = "Worker profile init failed",  .docs_uri = ERROR_DOCS_BASE ++ "UZ-WORKER-002" },
    // ── EXECUTOR ─────────────────────────────────────────────────────────────
    .{ .code = "UZ-EXEC-001", .http_status = .internal_server_error,.title = "Execution session create failed",.docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-001" },
    .{ .code = "UZ-EXEC-002", .http_status = .internal_server_error,.title = "Stage start failed",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-002" },
    .{ .code = "UZ-EXEC-003", .http_status = .internal_server_error,.title = "Execution timeout kill",         .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-003" },
    .{ .code = "UZ-EXEC-004", .http_status = .internal_server_error,.title = "Execution OOM kill",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-004" },
    .{ .code = "UZ-EXEC-005", .http_status = .internal_server_error,.title = "Execution resource kill",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-005" },
    .{ .code = "UZ-EXEC-006", .http_status = .internal_server_error,.title = "Execution transport loss",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-006" },
    .{ .code = "UZ-EXEC-007", .http_status = .internal_server_error,.title = "Execution lease expired",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-007" },
    .{ .code = "UZ-EXEC-008", .http_status = .forbidden,            .title = "Execution policy deny",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-008" },
    .{ .code = "UZ-EXEC-009", .http_status = .internal_server_error,.title = "Execution startup posture failure",.docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-009" },
    .{ .code = "UZ-EXEC-010", .http_status = .internal_server_error,.title = "Execution crash",                .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-010" },
    .{ .code = "UZ-EXEC-011", .http_status = .forbidden,            .title = "Landlock policy deny",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-011" },
    .{ .code = "UZ-EXEC-012", .http_status = .internal_server_error,.title = "Runner agent init failed",       .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-012" },
    .{ .code = "UZ-EXEC-013", .http_status = .internal_server_error,.title = "Runner agent run failed",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-013" },
    .{ .code = "UZ-EXEC-014", .http_status = .bad_request,          .title = "Runner invalid config",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-EXEC-014" },
    // ── RELAY ────────────────────────────────────────────────────────────────
    .{ .code = "UZ-RELAY-001", .http_status = .bad_request, .title = "No LLM provider configured", .docs_uri = ERROR_DOCS_BASE ++ "UZ-RELAY-001" },
    // ── CREDENTIALS ──────────────────────────────────────────────────────────
    .{ .code = "UZ-CRED-001", .http_status = .service_unavailable,.title = "Anthropic API key missing",    .docs_uri = ERROR_DOCS_BASE ++ "UZ-CRED-001" },
    .{ .code = "UZ-CRED-002", .http_status = .service_unavailable,.title = "GitHub token failed",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-CRED-002" },
    .{ .code = "UZ-CRED-003", .http_status = .service_unavailable,.title = "Platform LLM key missing",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-CRED-003" },
    // ── APPROVAL GATE ────────────────────────────────────────────────────────
    .{ .code = "UZ-APPROVAL-001", .http_status = .bad_request,          .title = "Approval parse failed",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-APPROVAL-001" },
    .{ .code = "UZ-APPROVAL-002", .http_status = .not_found,            .title = "Approval not found",             .docs_uri = ERROR_DOCS_BASE ++ "UZ-APPROVAL-002" },
    .{ .code = "UZ-APPROVAL-003", .http_status = .unauthorized,         .title = "Approval invalid signature",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-APPROVAL-003" },
    .{ .code = "UZ-APPROVAL-004", .http_status = .service_unavailable,  .title = "Approval Redis unavailable",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-APPROVAL-004" },
    .{ .code = "UZ-APPROVAL-005", .http_status = .bad_request,          .title = "Approval condition invalid",     .docs_uri = ERROR_DOCS_BASE ++ "UZ-APPROVAL-005" },
};

/// Look up an error entry by code string. Returns null if not found.
/// Callers should use UNKNOWN_ENTRY as the fallback:
///   const entry = error_table.lookup(code) orelse error_table.UNKNOWN_ENTRY;
pub fn lookup(code: []const u8) ?ErrorEntry {
    for (TABLE) |entry| {
        if (std.mem.eql(u8, entry.code, code)) return entry;
    }
    return null;
}

// Tests
test "lookup returns correct entry for ZMB-009 (zombie not found → 404)" {
    const entry = lookup("UZ-ZMB-009").?;
    try std.testing.expectEqual(std.http.Status.not_found, entry.http_status);
    try std.testing.expectEqualStrings("Zombie not found", entry.title);
    try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, ERROR_DOCS_BASE));
}

test "lookup returns correct entry for INTERNAL-001 (db unavailable → 503)" {
    const entry = lookup("UZ-INTERNAL-001").?;
    try std.testing.expectEqual(std.http.Status.service_unavailable, entry.http_status);
}

test "lookup returns correct entry for AUTH-002 (unauthorized → 401)" {
    const entry = lookup("UZ-AUTH-002").?;
    try std.testing.expectEqual(std.http.Status.unauthorized, entry.http_status);
    try std.testing.expectEqualStrings("Unauthorized", entry.title);
}

test "lookup returns correct entry for REQ-002 (payload too large → 413)" {
    const entry = lookup("UZ-REQ-002").?;
    try std.testing.expectEqual(std.http.Status.payload_too_large, entry.http_status);
}

test "lookup returns correct entry for PROPOSAL-017 (not found → 404)" {
    const entry = lookup("UZ-PROPOSAL-017").?;
    try std.testing.expectEqual(std.http.Status.not_found, entry.http_status);
}

test "lookup returns null for unknown code" {
    try std.testing.expectEqual(@as(?ErrorEntry, null), lookup("UZ-DOES-NOT-EXIST"));
}

test "UNKNOWN_ENTRY is 500 internal_server_error" {
    try std.testing.expectEqual(std.http.Status.internal_server_error, UNKNOWN_ENTRY.http_status);
}

test "UNKNOWN_ENTRY sentinel code is not in TABLE — no collision with registered codes" {
    for (TABLE) |entry| {
        const collides = std.mem.eql(u8, entry.code, UNKNOWN_ENTRY.code);
        try std.testing.expect(!collides);
    }
}

test "every entry has non-empty title and docs_uri starting with base" {
    for (TABLE) |entry| {
        try std.testing.expect(entry.title.len > 0);
        try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, ERROR_DOCS_BASE));
    }
}

test "all codes in TABLE are distinct — no duplicates" {
    for (TABLE, 0..) |outer, i| {
        for (TABLE, 0..) |inner, j| {
            if (i == j) continue;
            try std.testing.expect(!std.mem.eql(u8, outer.code, inner.code));
        }
    }
}
