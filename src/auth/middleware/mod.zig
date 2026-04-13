//! Public surface of the auth middleware layer (M18_002).
//!
//! Re-exports the chain runner and the concrete middleware implementations.
//! Callers import this module, construct the middleware structs they need,
//! and attach each via its `.middleware()` method to route specs.
//!
//! Batch B.1 ships bearer + admin + role gates. Batch B.2 adds webhook
//! HMAC/URL-secret middlewares. Batch B.3 adds Slack + OAuth.

pub const chain = @import("chain.zig");
pub const auth_ctx = @import("auth_ctx.zig");
pub const errors = @import("errors.zig");

pub const Middleware = chain.Middleware;
pub const Outcome = chain.Outcome;
pub const run = chain.run;

pub const AuthCtx = auth_ctx.AuthCtx;
pub const WriteErrorFn = auth_ctx.WriteErrorFn;

pub const admin_api_key = @import("admin_api_key.zig");
pub const bearer_oidc = @import("bearer_oidc.zig");
pub const bearer_or_api_key = @import("bearer_or_api_key.zig");
pub const require_role = @import("require_role.zig");

pub const AdminApiKey = admin_api_key.AdminApiKey;
pub const BearerOidc = bearer_oidc.BearerOidc;
pub const BearerOrApiKey = bearer_or_api_key.BearerOrApiKey;
pub const RequireRole = require_role.RequireRole;
