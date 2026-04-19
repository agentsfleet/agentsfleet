const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_validate = @import("config_validate.zig");
const config_types = @import("config_types.zig");

const ZombieConfigError = config_types.ZombieConfigError;

test "validateZombieSkills: unknown skill returns UnknownSkill" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name": "x", "trigger": {"type": "api"}, "skills": ["unknown_tool"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(
        ZombieConfigError.UnknownSkill,
        config_parser.parseZombieConfig(alloc, json),
    );
}

test "parseZombieConfig: credential names validated (no op:// paths)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","trigger":{"type":"api"},"skills":["agentmail"],
        \\ "credentials":["op://ZMB_LOCAL_DEV/agentmail/api_key"],
        \\ "budget":{"daily_dollars":1.0}}
    ;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_parser.parseZombieConfig(alloc, json),
    );
}

test "validateSkillsAndCredentials: empty cred name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateSkillsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{""},
        ),
    );
}

test "validateSkillsAndCredentials: 129-char cred name rejected" {
    const long_name = "a" ** 129;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateSkillsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{long_name},
        ),
    );
}

test "validateSkillsAndCredentials: dash in cred name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateSkillsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{"has-dash"},
        ),
    );
}

test "validateSkillsAndCredentials: alphanumeric + underscore accepted" {
    try config_validate.validateSkillsAndCredentials(
        &[_][]const u8{"agentmail"},
        &[_][]const u8{ "api_key_1", "SECRET_123" },
    );
}
