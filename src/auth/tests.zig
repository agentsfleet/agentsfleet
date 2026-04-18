//! Aggregate entry for the `auth-only-tests` build target (M18_002 §1.3).
//!
//! Linking only this file compiles every module under `src/auth/**` in
//! isolation — proving the portability contract: nothing here reaches
//! into `src/http/`, `src/state/`, `src/db/`, `src/observability/`, or
//! any other business-layer module. A standalone `zombie-auth` repo can
//! ship the same sources with zero edits.

test {
    _ = @import("api_key.zig");
    _ = @import("claims.zig");
    _ = @import("clerk.zig");
    _ = @import("github.zig");
    _ = @import("jwks.zig");
    _ = @import("oidc.zig");
    _ = @import("principal.zig");
    _ = @import("rbac.zig");
    _ = @import("sessions.zig");
    _ = @import("middleware/chain.zig");
    _ = @import("middleware/mod.zig");
    _ = @import("middleware/auth_ctx.zig");
    _ = @import("middleware/bearer.zig");
    _ = @import("middleware/errors.zig");
    _ = @import("middleware/admin_api_key.zig");
    _ = @import("middleware/bearer_oidc.zig");
    _ = @import("middleware/bearer_or_api_key.zig");
    _ = @import("middleware/require_role.zig");
    _ = @import("middleware/webhook_hmac.zig");
    _ = @import("middleware/webhook_url_secret.zig");
    _ = @import("middleware/slack_signature.zig");
    _ = @import("middleware/oauth_state.zig");
    _ = @import("middleware/webhook_sig.zig");
    _ = @import("middleware/svix_signature.zig");
}
