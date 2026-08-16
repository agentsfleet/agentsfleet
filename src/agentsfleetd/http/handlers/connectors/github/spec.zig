//! GitHub connector descriptor + provider id — the data the connector
//! registry's `app_install` archetype runs. GitHub is a GitHub App
//! installation whose user-authorization code proves access before agentsfleet
//! stores the installation handle the credential broker mints from.

const connector_state = @import("../state.zig");
const oauth2 = @import("../oauth2.zig");

/// Single source of the GitHub provider id (the `{provider}` route segment +
/// the `github-app`/`fleet:github` vault-key stem). Aliased from `common`
/// (RULE UFS).
pub const PROVIDER = @import("common").PROVIDER_GITHUB;

/// Install-state domain binding — the same signed single-use state mechanism
/// every connector uses, pinned to GitHub's domain + nonce namespace.
pub const STATE = connector_state.Config{
    .domain_prefix = "ghconnect:v1:",
    .nonce_prefix = "connect:gh:nonce:",
};

/// GitHub App user authorization uses the App's permissions, not OAuth scopes.
pub const USER_AUTH = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = "https://github.com/login/oauth/authorize",
    .token_endpoint = "https://github.com/login/oauth/access_token",
    .scopes = "",
    .state = STATE,
};
