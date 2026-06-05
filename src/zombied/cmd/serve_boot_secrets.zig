//! Boot-time secret resolution.
//!
//! `common.env.owned` returns `Allocator.Error!?[]const u8` — a two-axis result
//! where `null` means "unset" and an error means the owning dupe failed. Reading
//! these secrets with `catch null` collapses both axes, so an out-of-memory at
//! boot would masquerade as a missing secret: the server starts yet rejects every
//! webhook / Clerk-authenticated request, with no signal at the boot site.
//!
//! `resolve` keeps the axes apart — `null` only for a genuinely-unset var, and
//! the allocation error propagated so the caller fails the whole boot closed
//! (`main` turns it into a fatal exit). The Clerk API-key env name lives with its
//! consumer in `clerk_backend.SECRET_ENV_VAR`; the webhook/approval names have no
//! other owner, so they live here next to the resolver that reads them.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.zombied);

const EnvMap = common.env.Map;

const S_STARTUP_SECRET_ALLOC_FAILED = "startup.secret_alloc_failed";

pub const CLERK_WEBHOOK_SECRET_ENV = "CLERK_WEBHOOK_SECRET";
pub const APPROVAL_SIGNING_SECRET_ENV = "APPROVAL_SIGNING_SECRET";

/// Resolve an optional boot secret from `env_map`. Returns an owned copy the
/// caller must free, `null` when the var is genuinely unset, or propagates
/// `error.OutOfMemory` so boot fails closed — never masks the allocation
/// failure as "unset".
pub fn resolve(env_map: *const EnvMap, alloc: std.mem.Allocator, key: []const u8) std.mem.Allocator.Error!?[]const u8 {
    return common.env.owned(env_map, alloc, key) catch |e| {
        log.err(S_STARTUP_SECRET_ALLOC_FAILED, .{ .key = key, .error_code = error_codes.ERR_STARTUP_SECRET_RESOLVE });
        return e;
    };
}
