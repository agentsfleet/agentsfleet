//! HTTP handler facade — minimal surface after M18_002 Batch D.
//!
//! All routes are dispatched through route_table.zig → handler files directly.
//! The only symbols still needed by server.zig are Context, the skill_secret
//! helpers (before router.match), and the health tests referenced in this file.

const std = @import("std");
const common = @import("handlers/common.zig");
const skill_secret_handlers = @import("handlers/skill_secrets.zig");
const health_handlers = @import("handlers/health.zig");
const skill_secrets_http = @import("handlers/skill_secrets_http.zig");

pub const Context = common.Context;
pub const SkillSecretRoute = skill_secret_handlers.Route;

// Skill-secret routes are matched before router.match() in server.zig.
// They are NOT in route_table.zig (3-param routes not in the Route enum).
pub const handlePutWorkspaceSkillSecret = skill_secrets_http.handlePutWorkspaceSkillSecret;
pub const handleDeleteWorkspaceSkillSecret = skill_secrets_http.handleDeleteWorkspaceSkillSecret;

pub fn parseSkillSecretRoute(path: []const u8) ?SkillSecretRoute {
    return skill_secret_handlers.parseRoute(path);
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_ok = false,
    }));
}

test "integration: ready decision fails closed when db is unhealthy" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = false,
        .queue_ok = true,
    }));
}

test "integration: ready decision passes when dependencies are healthy" {
    try std.testing.expect(health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_ok = true,
    }));
}
