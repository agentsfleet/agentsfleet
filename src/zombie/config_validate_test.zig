const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_validate = @import("config_validate.zig");
const config_types = @import("config_types.zig");

const ZombieConfigError = config_types.ZombieConfigError;

test "validateZombieTools: unknown tool returns UnknownSkill" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{"trigger":{"type":"api"},"tools":["unknown_tool"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(
        ZombieConfigError.UnknownSkill,
        config_parser.parseZombieConfig(alloc, json),
    );
}

test "parseZombieConfig: credential names validated (no op:// paths)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],
        \\ "credentials":["op://ZMB_LOCAL_DEV/agentmail/api_key"],
        \\ "budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_parser.parseZombieConfig(alloc, json),
    );
}

test "validateToolsAndCredentials: empty cred name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateToolsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{""},
        ),
    );
}

test "validateToolsAndCredentials: 129-char cred name rejected" {
    const long_name = "a" ** 129;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateToolsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{long_name},
        ),
    );
}

test "validateToolsAndCredentials: dash in cred name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateToolsAndCredentials(
            &[_][]const u8{"agentmail"},
            &[_][]const u8{"has-dash"},
        ),
    );
}

test "validateToolsAndCredentials: alphanumeric + underscore accepted" {
    try config_validate.validateToolsAndCredentials(
        &[_][]const u8{"agentmail"},
        &[_][]const u8{ "api_key_1", "SECRET_123" },
    );
}

// ── validateSkillName ──────────────────────────────────────────────────────

test "validateSkillName: kebab slug accepted" {
    try config_validate.validateSkillName("platform-ops-zombie");
    try config_validate.validateSkillName("a");
    try config_validate.validateSkillName("z9");
}

test "validateSkillName: empty rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName(""),
    );
}

test "validateSkillName: 65 chars rejected (over MAX_NAME_LEN)" {
    const sixty_five = "a" ** 65;
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName(sixty_five),
    );
}

test "validateSkillName: 64 chars accepted (boundary)" {
    const sixty_four = "a" ** 64;
    try config_validate.validateSkillName(sixty_four);
}

test "validateSkillName: uppercase rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName("Foo-bar"),
    );
}

test "validateSkillName: underscore rejected (kebab not snake)" {
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName("foo_bar"),
    );
}

test "validateSkillName: space rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName("foo bar"),
    );
}

test "validateSkillName: dot rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidNameFormat,
        config_validate.validateSkillName("foo.bar"),
    );
}

// ── validateSkillVersion ───────────────────────────────────────────────────

test "validateSkillVersion: standard semver accepted" {
    try config_validate.validateSkillVersion("0.1.0");
    try config_validate.validateSkillVersion("1.2.3");
    try config_validate.validateSkillVersion("10.20.30");
    try config_validate.validateSkillVersion("0.0.0");
}

test "validateSkillVersion: empty rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion(""),
    );
}

test "validateSkillVersion: missing patch rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.2"),
    );
}

test "validateSkillVersion: four parts rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.2.3.4"),
    );
}

test "validateSkillVersion: leading zero rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("01.2.3"),
    );
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.02.3"),
    );
}

test "validateSkillVersion: bare zero per part accepted" {
    try config_validate.validateSkillVersion("0.0.1");
}

test "validateSkillVersion: prerelease suffix rejected (not yet supported)" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.2.3-alpha"),
    );
}

test "validateSkillVersion: empty part rejected (1..3)" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1..3"),
    );
}

test "validateSkillVersion: trailing dot rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.2.3."),
    );
}

test "validateSkillVersion: non-digit rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidVersionFormat,
        config_validate.validateSkillVersion("1.2.x"),
    );
}
