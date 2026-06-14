//! Uniform environment resolution policy.
//!
//! `common.env.owned` returns `Allocator.Error!?[]const u8` — a two-axis result
//! where `null` means "unset" and an error means the owning dupe failed. Reading
//! an env var with `catch null` collapses both axes, so an out-of-memory would
//! masquerade as a missing value with no signal at all.
//!
//! Both policies return `null` for a genuinely-unset var — they differ ONLY in
//! how they treat an allocation failure (named for the use class, not for
//! value-presence):
//!   - `secret` — fail closed. Propagates the allocation error so the caller
//!     aborts boot (`main` turns it into a fatal exit). For secrets, where
//!     booting without one silently breaks authentication. (Unset still returns
//!     `null`; the consumer rejects requests per-request.)
//!   - `config` — safe fallback. Logs a warning on the allocation failure (so a
//!     silent OOM can't masquerade as "unset") and returns `null`; stays quiet
//!     for a genuinely-unset var. For config/telemetry that has a sane default
//!     and must never take the server down.
//!
//! The Clerk API-key env name lives with its consumer in
//! `clerk_backend.SECRET_ENV_VAR`; the webhook/approval names have no other
//! owner, so they live here next to the resolver that reads them.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.agentsfleetd);

const EnvMap = common.env.Map;

const S_SECRET_ALLOC_FAILED = "startup.secret_alloc_failed";
const S_CONFIG_ALLOC_FAILED = "startup.config_alloc_failed";

pub const CLERK_WEBHOOK_SECRET_ENV = "CLERK_WEBHOOK_SECRET";
pub const APPROVAL_SIGNING_SECRET_ENV = "APPROVAL_SIGNING_SECRET";

/// Fail-closed resolution for secrets. Returns an owned copy the caller must
/// free, `null` when the var is genuinely unset, or propagates
/// `error.OutOfMemory` so boot fails closed — never masks the allocation
/// failure as "unset".
pub fn secret(env_map: *const EnvMap, alloc: std.mem.Allocator, key: []const u8) std.mem.Allocator.Error!?[]const u8 {
    return common.env.owned(env_map, alloc, key) catch |e| {
        log.err(S_SECRET_ALLOC_FAILED, .{ .key = key, .error_code = error_codes.ERR_STARTUP_ENV_ALLOC });
        return e;
    };
}

/// Safe-fallback resolution for config/telemetry. Returns an owned copy the
/// caller must free or `null`. Logs a warning on an allocation failure so a
/// silent OOM can't masquerade as "unset"; stays quiet when the var is
/// genuinely unset. The caller substitutes its default for `null`.
pub fn config(env_map: *const EnvMap, alloc: std.mem.Allocator, key: []const u8) ?[]const u8 {
    return common.env.owned(env_map, alloc, key) catch {
        log.warn(S_CONFIG_ALLOC_FAILED, .{ .key = key, .error_code = error_codes.ERR_STARTUP_ENV_ALLOC });
        return null;
    };
}
