//! Public surface of the auth middleware layer (M18_002).
//!
//! Re-exports the chain runner and — once concrete middlewares land in
//! Batch B — the bearer / api-key / role / webhook / oauth implementations.
//! Callers import this module and compose policies via `policies.*`.

pub const chain = @import("chain.zig");

pub const Middleware = chain.Middleware;
pub const Outcome = chain.Outcome;
pub const run = chain.run;

/// Pre-defined middleware chains callers attach to routes.
///
/// Batch A ships only `none`. Concrete entries (bearer, admin, operator,
/// webhook_hmac, webhook_secret, slack, oauth_callback) land in Batch B
/// once their backing middleware files exist.
pub const policies = struct {
    /// Route opts out of auth (e.g. login endpoint, public health check).
    pub fn none(comptime Ctx: type) []const chain.Middleware(Ctx) {
        return &.{};
    }
};
