// Unit tests for handler auth primitives (scope-based authz).
//
// The role ladder (`AuthRole`/`requireRole`/`Requirement.minimum_role`) was
// removed — capability now rides `principal.scopes`, enforced by
// the requireScope middleware, and `workspace_guards.enforce` is ownership-only.
// This file covers the pure/compile-time-verifiable aspects that survive.

const std = @import("std");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const error_codes = @import("../../errors/error_registry.zig");
const BYTES_PER_KIB = 1024;

// --- AuthPrincipal default scope set ---

test "AuthPrincipal defaults to an empty scope set (fail-closed)" {
    const principal = common.AuthPrincipal{ .mode = .jwt_oidc };
    try std.testing.expectEqual(@as(usize, 0), principal.scopes.count());
}

test "AuthPrincipal carries an explicit scope set" {
    const scopes = @import("../../auth/scopes.zig");
    const principal = common.AuthPrincipal{
        .mode = .api_key,
        .scopes = scopes.parseClaim("fleet:admin"),
    };
    try std.testing.expect(principal.scopes.contains(.fleet_admin));
    try std.testing.expect(principal.scopes.contains(.fleet_read)); // closure
}

// --- workspace_guards.Access (ownership token; role Requirement removed) ---

test "workspace_guards.Access.deinit is a no-op" {
    const access = workspace_guards.Access{};
    access.deinit(std.testing.allocator);
}

test "workspace_guards module imports resolve" {
    _ = @import("../workspace_guards.zig");
}

// --- scope-denied error code ---

test "ERR_INSUFFICIENT_SCOPE error code is UZ-AUTH-022" {
    try std.testing.expectEqualStrings("UZ-AUTH-022", error_codes.ERR_INSUFFICIENT_SCOPE);
}

// --- MAX_BODY_SIZE constant ---

test "MAX_BODY_SIZE is 2 MB" {
    try std.testing.expectEqual(@as(usize, 2 * BYTES_PER_KIB * BYTES_PER_KIB), common.MAX_BODY_SIZE);
}

// --- API_ACTOR fallback ---

test "API_ACTOR fallback produces expected sentinel value" {
    const principal = common.AuthPrincipal{ .mode = .jwt_oidc };
    const actor = principal.user_id orelse "api";
    try std.testing.expectEqualStrings("api", actor);
}
