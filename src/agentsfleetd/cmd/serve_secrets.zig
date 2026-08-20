//! Boot-time resolution of the webhook/backend secrets. Handlers and the webhook
//! middleware borrow these for the process lifetime instead of re-reading env per
//! request; null = unset → the consumer fails closed. Split out of serve.zig to
//! keep that file within the line cap.

const std = @import("std");
const common = @import("common");
const env_resolve = @import("../config/env_resolve.zig");
const clerk_backend = @import("../auth/clerk_backend.zig");

const EnvMap = common.env.Map;

/// Owned boot secrets; `deinit` frees each present value. The owning scope must
/// outlive every borrow (Context fields + the webhook middleware).
pub const Secrets = struct {
    clerk_webhook_secret: ?[]const u8,
    approval_signing_secret: ?[]const u8,
    clerk_secret_key: ?[]const u8,
    alloc: std.mem.Allocator,

    pub fn deinit(self: *Secrets) void {
        if (self.clerk_webhook_secret) |s| self.alloc.free(s);
        if (self.approval_signing_secret) |s| self.alloc.free(s);
        if (self.clerk_secret_key) |s| self.alloc.free(s);
        self.* = undefined;
    }
};

/// Resolve all boot secrets from the env. Propagates OutOfMemory so boot fails
/// closed; a genuinely unset var resolves to null.
pub fn resolve(env_map: *const EnvMap, alloc: std.mem.Allocator) std.mem.Allocator.Error!Secrets {
    const clerk_webhook = try env_resolve.secret(env_map, alloc, env_resolve.CLERK_WEBHOOK_SECRET_ENV);
    errdefer if (clerk_webhook) |s| alloc.free(s);
    const approval_signing = try env_resolve.secret(env_map, alloc, env_resolve.APPROVAL_SIGNING_SECRET_ENV);
    errdefer if (approval_signing) |s| alloc.free(s);
    const clerk_key = try env_resolve.secret(env_map, alloc, clerk_backend.SECRET_ENV_VAR);
    return .{
        .clerk_webhook_secret = clerk_webhook,
        .approval_signing_secret = approval_signing,
        .clerk_secret_key = clerk_key,
        .alloc = alloc,
    };
}

fn resolveUnderAllocator(alloc: std.mem.Allocator) !void {
    var env_map = try common.env.fromPairs(alloc, &.{
        .{ env_resolve.CLERK_WEBHOOK_SECRET_ENV, "whsec_proof_value" },
        .{ env_resolve.APPROVAL_SIGNING_SECRET_ENV, "approval_proof_value" },
        .{ clerk_backend.SECRET_ENV_VAR, "sk_proof_value" },
    });
    defer env_map.deinit();
    var secrets = try resolve(&env_map, alloc);
    secrets.deinit();
}

test "test_boot_secrets_resolve_unwinds_without_leaking" {
    // All three vars are set, so every rung holds a real allocation rather than
    // the null an unset var leaves behind — a rung that frees the wrong
    // variable is invisible when the value it guards is null. Boot is the one
    // path here, and a leak on it is charged to the process for its lifetime.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, resolveUnderAllocator, .{});
}
