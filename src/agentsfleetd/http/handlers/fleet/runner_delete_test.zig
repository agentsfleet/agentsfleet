const std = @import("std");
const protocol = @import("contract").protocol;
const ec = @import("../../../errors/error_registry.zig");

// The delete guard is expressed in SQL as `admin_state = $2` with $2 bound to
// @tagName(protocol.AdminState.revoked). That couples a database predicate to a
// Zig enum spelling with nothing in between to catch a rename, so pin it: if
// the tag is ever renamed, this fails loudly here rather than silently turning
// the guard into "matches no row" and making every delete a 409.
test "revoked tag name matches the admin_state value the delete guard binds" {
    try std.testing.expectEqualStrings("revoked", @tagName(protocol.AdminState.revoked));
}

test "revoke-first conflict is a registered 409 with operator-facing copy" {
    const entry = ec.lookup(ec.ERR_RUNNER_MUST_REVOKE_FIRST);
    try std.testing.expectEqualStrings("UZ-RUN-016", entry.code);
    try std.testing.expectEqual(std.http.Status.conflict, entry.http_status);
    // Dashboard-reachable: the operator hits this by clicking delete on a runner
    // that is not revoked, so it must carry a user_message rather than only a
    // developer hint.
    try std.testing.expect(entry.user_message != null);
}

test "runner-not-found stays distinct from the revoke-first conflict" {
    // Two different outcomes of the same request must not collapse onto one
    // code: "no such runner" is a 404, "present but still active" is a 409.
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_RUNNER_NOT_FOUND, ec.ERR_RUNNER_MUST_REVOKE_FIRST));
    try std.testing.expectEqual(std.http.Status.not_found, ec.lookup(ec.ERR_RUNNER_NOT_FOUND).http_status);
}
