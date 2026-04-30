const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_validate = @import("config_validate.zig");
const config_types = @import("config_types.zig");

const ZombieConfigError = config_types.ZombieConfigError;

test "parseZombieConfig: credential names validated (no op:// paths)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","trigger":{"type":"api"},"tools":["agentmail"],
        \\ "credentials":["op://ZMB_LOCAL_DEV/agentmail/api_key"],
        \\ "budget":{"daily_dollars":1.0}}
    ;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_parser.parseZombieConfig(alloc, json),
    );
}

test "validateCredentials: empty name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateCredentials(&[_][]const u8{""}),
    );
}

test "validateCredentials: 129-char name rejected" {
    const long_name = "a" ** 129;
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateCredentials(&[_][]const u8{long_name}),
    );
}

test "validateCredentials: dash in name rejected" {
    try std.testing.expectError(
        ZombieConfigError.InvalidCredentialRef,
        config_validate.validateCredentials(&[_][]const u8{"has-dash"}),
    );
}

test "validateCredentials: alphanumeric + underscore accepted" {
    try config_validate.validateCredentials(&[_][]const u8{ "api_key_1", "SECRET_123" });
}

test "parseZombieConfig: unknown tool name accepted (gate moved to executor sandbox)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","trigger":{"type":"api"},"tools":["whatever_the_skill_wants"],"budget":{"daily_dollars":1.0}}
    ;
    var cfg = try config_parser.parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("whatever_the_skill_wants", cfg.tools[0]);
}
