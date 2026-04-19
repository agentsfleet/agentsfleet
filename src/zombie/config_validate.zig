// Zombie config validation.
//
// Pure predicate helpers — no allocation, no I/O. Each returns `void` on
// success or the specific ZombieConfigError variant on failure.

const std = @import("std");
const config_types = @import("config_types.zig");
const helpers = @import("config_helpers.zig");

const ZombieConfig = config_types.ZombieConfig;
const ZombieConfigError = config_types.ZombieConfigError;

const log = std.log.scoped(.zombie_config);

const MAX_CREDENTIAL_NAME_LEN: usize = 128;

/// Fast path for config upload: validates skill+credential names without
/// touching allocators. Both arrays borrowed — caller retains ownership.
pub fn validateSkillsAndCredentials(
    skills: []const []const u8,
    credentials: []const []const u8,
) ZombieConfigError!void {
    for (skills) |skill| {
        if (!helpers.isKnownZombieSkill(skill)) return ZombieConfigError.UnknownSkill;
    }
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

/// Upload-time check against the known skill registry. Logs the offending
/// name so the developer sees an actionable message in server logs.
pub fn validateZombieSkills(config: ZombieConfig) ZombieConfigError!void {
    for (config.skills) |skill| {
        if (!helpers.isKnownZombieSkill(skill)) {
            log.warn("zombie_config.validate unknown_skill={s}", .{skill});
            return ZombieConfigError.UnknownSkill;
        }
    }
}
