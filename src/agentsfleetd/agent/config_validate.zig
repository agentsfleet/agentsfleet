// Agent config validation.
//
// Pure predicate helpers — no allocation, no I/O. Each returns `void` on
// success or the specific AgentConfigError variant on failure.

const std = @import("std");
const config_types = @import("config_types.zig");

const AgentConfigError = config_types.AgentConfigError;

const MAX_CREDENTIAL_NAME_LEN: usize = 128;

/// Validate credential name shapes. Tool names are not gated at parse
/// time — the runner sandbox is the binding authority on tool dispatch,
/// and a parse-time tool allowlist drifts faster than the binary ships.
pub fn validateCredentials(
    credentials: []const []const u8,
) AgentConfigError!void {
    for (credentials) |cred| {
        try validateCredentialName(cred);
    }
}

fn validateCredentialName(cred: []const u8) AgentConfigError!void {
    if (cred.len == 0 or cred.len > MAX_CREDENTIAL_NAME_LEN) {
        return AgentConfigError.InvalidCredentialRef;
    }
    for (cred) |c| {
        if (!std.ascii.isAlphanumeric(c) and c != '_') {
            return AgentConfigError.InvalidCredentialRef;
        }
    }
}
const MAX_NAME_LEN: usize = 64;

/// SKILL.md / TRIGGER.md `name:` shape — kebab slug `^[a-z0-9-]+$` with
/// length 1..=64. Mirrors the regex from `docs/SKILL_FRONTMATTER_SCHEMA.md`.
/// Enforced at install so invalid names fail loud at the boundary instead
/// of leaking into URLs, log scopes, or DB keys downstream.
pub fn validateSkillName(name: []const u8) AgentConfigError!void {
    if (name.len == 0 or name.len > MAX_NAME_LEN) return AgentConfigError.InvalidNameFormat;
    for (name) |c| {
        const ok = (c >= 'a' and c <= 'z') or (c >= '0' and c <= '9') or c == '-';
        if (!ok) return AgentConfigError.InvalidNameFormat;
    }
}

/// SKILL.md `version:` shape — basic semver `MAJOR.MINOR.PATCH` where each
/// component is a non-empty digit string with no leading zero (except "0"
/// itself). Pre-release/build suffixes (`-alpha`, `+build`) intentionally
/// not supported yet — author can add when a consumer needs them.
pub fn validateSkillVersion(version: []const u8) AgentConfigError!void {
    if (version.len == 0) return AgentConfigError.InvalidVersionFormat;
    var part_count: usize = 0;
    var part_len: usize = 0;
    var part_first: u8 = 0;
    for (version) |c| {
        if (c == '.') {
            if (part_len == 0) return AgentConfigError.InvalidVersionFormat;
            if (part_len > 1 and part_first == '0') return AgentConfigError.InvalidVersionFormat;
            part_count += 1;
            part_len = 0;
            continue;
        }
        if (c < '0' or c > '9') return AgentConfigError.InvalidVersionFormat;
        if (part_len == 0) part_first = c;
        part_len += 1;
    }
    if (part_len == 0) return AgentConfigError.InvalidVersionFormat;
    if (part_len > 1 and part_first == '0') return AgentConfigError.InvalidVersionFormat;
    part_count += 1;
    if (part_count != 3) return AgentConfigError.InvalidVersionFormat;
}
