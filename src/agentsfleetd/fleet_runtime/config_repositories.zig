//! The fleet's repository EGRESS binding — `x-agentsfleet.repositories` plus
//! `x-agentsfleet.repository_access`. Split from `config_parser.zig` for the
//! file-length budget (RULE FLL).
//!
//! This is the binding the GitHub mint scopes a token by, so its parse rules are
//! deliberately strict: a half-declared or empty binding is an authoring error,
//! never a permissive default.

const std = @import("std");
const Allocator = std.mem.Allocator;

const config_types = @import("config_types.zig");
const helpers = @import("config_helpers.zig");

const FleetConfigError = config_types.FleetConfigError;

/// Authoring rejects incomplete write reach. Stored parsing admits only the
/// one pre-base shape so the daemon can surface a durable upgrade refusal.
pub const ParseMode = enum { authoring, stored };

pub const S_REPOSITORIES = "repositories";
pub const S_REPOSITORY_ACCESS = "repository_access";
pub const S_REPOSITORY_BASE = "repository_base";
const MAX_BASE_BRANCH_LEN: usize = 255;

/// Parse the top-level repository EGRESS binding — `repositories` plus
/// `repository_access` — into a single optional. Both keys are optional, but
/// they are optional TOGETHER: declaring one without the other is an authoring
/// error rather than a half-binding, because a half-binding has no safe reading.
/// A list without an access level does not know how far to reach, and an access
/// level without a list does not know what to reach; either would have to fall
/// back to the installation's full scope, which is exactly what the binding
/// exists to prevent. Absent entirely → null → the GitHub mint refuses.
pub fn parse(
    alloc: Allocator,
    runtime: std.json.ObjectMap,
    mode: ParseMode,
) (Allocator.Error || FleetConfigError)!?config_types.RepositoryBinding {
    const repos_val = runtime.get(S_REPOSITORIES);
    const access_val = runtime.get(S_REPOSITORY_ACCESS);
    const base_val = runtime.get(S_REPOSITORY_BASE);
    if (repos_val == null and access_val == null and base_val == null) return null;
    if (repos_val == null or access_val == null) return FleetConfigError.MissingRequiredField;

    const arr = switch (repos_val.?) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    // An empty list is not "every repository" — it is a binding that names
    // nothing, and a mint scoped to nothing cannot succeed. Reject at authoring.
    if (arr.items.len == 0) return FleetConfigError.MissingRequiredField;

    const access = switch (access_val.?) {
        .string => |s| config_types.RepositoryAccess.fromSlice(s) orelse return FleetConfigError.MissingRequiredField,
        else => return FleetConfigError.MissingRequiredField,
    };

    const base_branch = try parseBaseBranch(alloc, access, base_val, mode);
    errdefer if (base_branch) |base| alloc.free(base);
    const repositories = try helpers.dupeStringArray(alloc, arr.items);
    return .{ .repositories = repositories, .access = access, .base_branch = base_branch };
}

fn parseBaseBranch(
    alloc: Allocator,
    access: config_types.RepositoryAccess,
    value: ?std.json.Value,
    mode: ParseMode,
) (Allocator.Error || FleetConfigError)!?[]const u8 {
    if (access == .read) {
        if (value != null) return FleetConfigError.InvalidFieldType;
        return null;
    }
    if (value == null and mode == .stored) return null;
    const field = value orelse return FleetConfigError.MissingRequiredField;
    const base = switch (field) {
        .string => |text| text,
        else => return FleetConfigError.InvalidFieldType,
    };
    if (!validBaseBranch(base)) return FleetConfigError.InvalidFieldType;
    return try alloc.dupe(u8, base);
}

fn validBaseBranch(base: []const u8) bool {
    if (base.len == 0 or base.len > MAX_BASE_BRANCH_LEN) return false;
    if (base[0] == '/' or base[base.len - 1] == '/' or base[base.len - 1] == '.') return false;
    if (std.mem.indexOf(u8, base, "..") != null or
        std.mem.indexOf(u8, base, "//") != null or
        std.mem.indexOf(u8, base, "@{") != null or
        std.mem.endsWith(u8, base, ".lock")) return false;
    for (base) |byte| {
        if (byte < 0x21 or byte == 0x7f or std.mem.indexOfScalar(u8, "~^:?*[\\", byte) != null) return false;
    }
    return true;
}
