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

pub const S_REPOSITORIES = "repositories";
pub const S_REPOSITORY_ACCESS = "repository_access";

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
) (Allocator.Error || FleetConfigError)!?config_types.RepositoryBinding {
    const repos_val = runtime.get(S_REPOSITORIES);
    const access_val = runtime.get(S_REPOSITORY_ACCESS);
    if (repos_val == null and access_val == null) return null;
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

    const repositories = try helpers.dupeStringArray(alloc, arr.items);
    return .{ .repositories = repositories, .access = access };
}
