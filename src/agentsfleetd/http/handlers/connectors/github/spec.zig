//! GitHub connector descriptor + provider id — the data the connector
//! registry's `app_install` archetype runs. GitHub is a GitHub App
//! *installation*, not an OAuth-2.0 code exchange: the callback carries an
//! `installation_id` (no `code`, nothing to exchange) and writes only the
//! vault handle the credential broker mints from.

const connector_state = @import("../state.zig");

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
