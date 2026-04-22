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

pub fn printValidationError(err: ValidationError) void {
    switch (err) {
        ValidationError.OidcRequired => std.debug.print("fatal: OIDC is required — set OIDC_JWKS_URL, OIDC_ISSUER, OIDC_AUDIENCE\n", .{}),
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
