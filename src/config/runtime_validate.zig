// Runtime config validation + human-readable error printer.
//
// Pure predicate helpers (hex + api-key list), plus printValidationError
// which fatals to stderr when load() returns a ValidationError. The printer
// is kept here so adding a new ValidationError variant requires updating
// one file — the switch gives a compile-time reminder when a case is missed.

const std = @import("std");
const oidc = @import("../auth/oidc.zig");
const runtime_types = @import("runtime_types.zig");

const ValidationError = runtime_types.ValidationError;

pub fn isHexString(s: []const u8) bool {
    for (s) |ch| {
        if (!std.ascii.isHex(ch)) return false;
    }
    return true;
}

/// True if the comma-separated API_KEY list has at least one non-empty
/// candidate (ignoring whitespace-only entries).
pub fn hasUsableApiKey(list: []const u8) bool {
    var it = std.mem.tokenizeScalar(u8, list, ',');
    while (it.next()) |candidate_raw| {
        if (std.mem.trim(u8, candidate_raw, " \t").len > 0) return true;
    }
    return false;
}

pub fn printValidationError(err: ValidationError) void {
    switch (err) {
        ValidationError.MissingApiKey => std.debug.print("fatal: API_KEY not set\n", .{}),
        ValidationError.InvalidApiKeyList => std.debug.print("fatal: API_KEY has no usable keys\n", .{}),
        ValidationError.MissingOidcJwksUrl => std.debug.print("fatal: OIDC_JWKS_URL is required and must be non-empty\n", .{}),
        ValidationError.InvalidOidcProvider => std.debug.print("fatal: OIDC_PROVIDER is invalid (supported: {s})\n", .{oidc.supportedProviderList()}),
        ValidationError.MissingEncryptionMasterKey => std.debug.print("fatal: ENCRYPTION_MASTER_KEY not set\n", .{}),
        ValidationError.InvalidEncryptionMasterKey => std.debug.print("fatal: ENCRYPTION_MASTER_KEY must be 64 hex chars\n", .{}),
        ValidationError.InvalidPort => std.debug.print("fatal: invalid PORT value\n", .{}),
        ValidationError.InvalidApiHttpThreads => std.debug.print("fatal: invalid API_HTTP_THREADS value\n", .{}),
        ValidationError.InvalidApiHttpWorkers => std.debug.print("fatal: invalid API_HTTP_WORKERS value\n", .{}),
        ValidationError.InvalidApiMaxClients => std.debug.print("fatal: invalid API_MAX_CLIENTS value\n", .{}),
        ValidationError.InvalidApiMaxInFlightRequests => std.debug.print("fatal: invalid API_MAX_IN_FLIGHT_REQUESTS value\n", .{}),
        ValidationError.InvalidReadyMaxQueueDepth => std.debug.print("fatal: invalid READY_MAX_QUEUE_DEPTH value\n", .{}),
        ValidationError.InvalidReadyMaxQueueAgeMs => std.debug.print("fatal: invalid READY_MAX_QUEUE_AGE_MS value\n", .{}),
        ValidationError.InvalidKekVersion => std.debug.print("fatal: KEK_VERSION must be 1 or 2\n", .{}),
        ValidationError.MissingEncryptionMasterKeyV2 => std.debug.print("fatal: ENCRYPTION_MASTER_KEY_V2 not set (required when KEK_VERSION=2)\n", .{}),
        ValidationError.InvalidEncryptionMasterKeyV2 => std.debug.print("fatal: ENCRYPTION_MASTER_KEY_V2 must be 64 hex chars\n", .{}),
    }
}
