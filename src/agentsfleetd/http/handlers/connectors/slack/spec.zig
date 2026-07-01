//! Slack connector descriptor + provider id — the data the shared OAuth-2.0
//! mechanism (`connectors/oauth2.zig`) runs. Adding Zoho/Jira/Linear is a
//! sibling `Spec` like this one, not new flow code.

const oauth2 = @import("../oauth2.zig");

/// Single source of the Slack provider id (the `provider` column value + the
/// `<provider>-app`/`fleet:<provider>` vault-key stem). Aliased from
/// `common` so the OAuth connector, the events ingress, and the migrations
/// all key off one constant (RULE UFS).
pub const PROVIDER = @import("common").PROVIDER_SLACK;

pub const SPEC = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = "https://slack.com/oauth/v2/authorize",
    .token_endpoint = "https://slack.com/api/oauth.v2.access",
    // Bot scopes: receive @mentions, post replies, and read the recent thread on
    // a mention (bounded re-read — whole-channel history is out of scope, spec
    // §Out of Scope).
    .scopes = "app_mentions:read,chat:write,channels:history",
    .state = .{
        .domain_prefix = "slackconnect:v1:",
        .nonce_prefix = "connect:slack:nonce:",
    },
};
