// Zombie config validation.
//
// Pure predicate helpers — no allocation, no I/O. Each returns `void` on
// success or the specific ZombieConfigError variant on failure.

const std = @import("std");
const config_types = @import("config_types.zig");

const ZombieConfigError = config_types.ZombieConfigError;

const MAX_CREDENTIAL_NAME_LEN: usize = 128;

/// Validate credential name shapes. Tool names are not gated at parse
/// time — the executor sandbox is the binding authority on tool dispatch,
/// and a parse-time tool allowlist drifts faster than the binary ships.
pub fn validateCredentials(
    credentials: []const []const u8,
) ZombieConfigError!void {
    for (credentials) |cred| {
        try validateCredentialName(cred);
    }
}

fn validateCredentialName(cred: []const u8) ZombieConfigError!void {
    if (cred.len == 0 or cred.len > MAX_CREDENTIAL_NAME_LEN) {
        return ZombieConfigError.InvalidCredentialRef;
    }
    for (cred) |c| {
        if (!std.ascii.isAlphanumeric(c) and c != '_') {
            return ZombieConfigError.InvalidCredentialRef;
        }
    }
}
