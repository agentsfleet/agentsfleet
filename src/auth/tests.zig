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
}
