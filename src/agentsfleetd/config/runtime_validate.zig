// Runtime config validation + human-readable error printer.
//
// Pure predicate helpers (hex + api-key list), plus printValidationError
// which fatals to stderr when load() returns a ValidationError. The printer
// is kept here so adding a new ValidationError variant requires updating
// one file — the switch gives a compile-time reminder when a case is missed.

const std = @import("std");
const oidc = @import("../auth/oidc.zig");
const runtime_types = @import("runtime_types.zig");
const logging = @import("log");

const ValidationError = runtime_types.ValidationError;

pub fn isHexString(s: []const u8) bool {
    for (s) |ch| {
        if (!std.ascii.isHex(ch)) return false;
    }
    return true;
}

pub fn printValidationError(err: ValidationError) void {
    switch (err) {
        ValidationError.OidcRequired => logging.fatalStderr("fatal: OIDC is required — set OIDC_ISSUER and OIDC_AUDIENCE (OIDC_JWKS_URL optional, derived from issuer)\n", .{}),
        ValidationError.MissingOidcIssuer => logging.fatalStderr("fatal: OIDC_ISSUER is required and must be non-empty\n", .{}),
        ValidationError.InvalidOidcProvider => logging.fatalStderr("fatal: OIDC_PROVIDER is invalid (supported: {s})\n", .{oidc.supportedProviderList()}),
        ValidationError.MissingEncryptionMasterKey => logging.fatalStderr("fatal: ENCRYPTION_MASTER_KEY not set\n", .{}),
        ValidationError.InvalidEncryptionMasterKey => logging.fatalStderr("fatal: ENCRYPTION_MASTER_KEY must be 64 hex chars\n", .{}),
        ValidationError.MissingAuthSessionCodePepper => logging.fatalStderr("fatal: AUTH_SESSION_CODE_PEPPER not set (see bootstrap playbook auth-pepper provisioning)\n", .{}),
        ValidationError.InvalidAuthSessionCodePepper => logging.fatalStderr("fatal: AUTH_SESSION_CODE_PEPPER must be 64 hex chars\n", .{}),
        ValidationError.MissingAuditLogPepper => logging.fatalStderr("fatal: AUDIT_LOG_PEPPER not set (see bootstrap playbook auth-pepper provisioning)\n", .{}),
        ValidationError.InvalidAuditLogPepper => logging.fatalStderr("fatal: AUDIT_LOG_PEPPER must be 64 hex chars\n", .{}),
        ValidationError.InvalidPort => logging.fatalStderr("fatal: invalid PORT value\n", .{}),
        ValidationError.InvalidApiHttpThreads => logging.fatalStderr("fatal: invalid API_HTTP_THREADS value\n", .{}),
        ValidationError.InvalidApiHttpWorkers => logging.fatalStderr("fatal: invalid API_HTTP_WORKERS value\n", .{}),
        ValidationError.InvalidApiMaxClients => logging.fatalStderr("fatal: invalid API_MAX_CLIENTS value\n", .{}),
        ValidationError.InvalidApiMaxInFlightRequests => logging.fatalStderr("fatal: invalid API_MAX_IN_FLIGHT_REQUESTS value\n", .{}),
        ValidationError.InvalidSseMaxStreams => logging.fatalStderr("fatal: invalid SSE_MAX_STREAMS value\n", .{}),
        ValidationError.InvalidReadyMaxQueueDepth => logging.fatalStderr("fatal: invalid READY_MAX_QUEUE_DEPTH value\n", .{}),
        ValidationError.InvalidReadyMaxQueueAgeMs => logging.fatalStderr("fatal: invalid READY_MAX_QUEUE_AGE_MS value\n", .{}),
    }
}
