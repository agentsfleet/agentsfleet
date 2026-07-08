//! auth_codes mirror-pin — split out of error_registry.zig to keep that file
//! under the 350-line cap (RULE FLL). Triggered via
//! `comptime { _ = @import("error_registry_mirror_pin.zig"); }` in
//! error_registry.zig: importing a file forces Zig to analyze its top-level
//! container, so the `comptime` block below still runs eagerly — same
//! mechanism the `test { _ = @import("foo_test.zig"); }` pattern relies on
//! throughout this codebase.
//!
//! The auth plane imports these codes via the `auth_codes` named module (it
//! cannot relative-import error_registry.zig without breaking the
//! test-auth portability gate). That leaf duplicates the literals; this pin
//! makes any drift a compile error.

const std = @import("std");
const registry = @import("error_registry.zig");
const auth_codes = @import("auth_codes");

comptime {
    const pairs = .{
        .{ registry.ERR_FORBIDDEN, auth_codes.ERR_FORBIDDEN },
        .{ registry.ERR_UNAUTHORIZED, auth_codes.ERR_UNAUTHORIZED },
        .{ registry.ERR_TOKEN_EXPIRED, auth_codes.ERR_TOKEN_EXPIRED },
        .{ registry.ERR_AUTH_UNAVAILABLE, auth_codes.ERR_AUTH_UNAVAILABLE },
        .{ registry.ERR_INSUFFICIENT_SCOPE, auth_codes.ERR_INSUFFICIENT_SCOPE },
        .{ registry.ERR_APPROVAL_INVALID_SIGNATURE, auth_codes.ERR_APPROVAL_INVALID_SIGNATURE },
        .{ registry.ERR_WEBHOOK_SIG_INVALID, auth_codes.ERR_WEBHOOK_SIG_INVALID },
        .{ registry.ERR_WEBHOOK_TIMESTAMP_STALE, auth_codes.ERR_WEBHOOK_TIMESTAMP_STALE },
        .{ registry.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, auth_codes.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED },
        .{ registry.ERR_APIKEY_REVOKED, auth_codes.ERR_APIKEY_REVOKED },
        .{ registry.ERR_RUN_INVALID_RUNNER_TOKEN, auth_codes.ERR_RUN_INVALID_RUNNER_TOKEN },
        .{ registry.ERR_RUN_ADMIN_STATE_BLOCKED, auth_codes.ERR_RUN_ADMIN_STATE_BLOCKED },
        .{ registry.ERR_INTERNAL_OPERATION_FAILED, auth_codes.ERR_INTERNAL_OPERATION_FAILED },
    };
    for (pairs) |p| {
        if (!std.mem.eql(u8, p[0], p[1]))
            @compileError("auth_codes mirror drift: " ++ p[0]);
    }
}
