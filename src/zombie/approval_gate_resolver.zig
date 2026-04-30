// Resolver-attribution prefixes for approval-gate resolutions.
//
// Every channel that calls approval_gate.resolve() writes the resolver
// identity as a `<channel>:<id>` string into core.zombie_approval_gates
// .resolved_by. The dashboard renders these verbatim — "already resolved
// by slack:webhook at 14:32" — so a rename of the channel string requires
// touching every writer. Centralising the literals here makes that mechanical:
// rename here, fix every consumer through the type system.

const std = @import("std");
const Allocator = std.mem.Allocator;

pub const SLACK_WEBHOOK = "slack:webhook";
pub const SLACK_INTERACTION = "slack:interaction";
pub const SYSTEM_TIMEOUT = "system:timeout";

const PREFIX_USER = "user:";
const PREFIX_API = "api:";

/// Build "user:<subject>". Caller owns the returned slice.
pub fn user(alloc: Allocator, subject: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ PREFIX_USER, subject });
}

/// Build "api:<key_id>". Caller owns the returned slice.
pub fn apiKey(alloc: Allocator, key_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ PREFIX_API, key_id });
}
