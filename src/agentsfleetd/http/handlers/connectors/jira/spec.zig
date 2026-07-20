//! Jira Cloud OAuth connector descriptor. Atlassian OAuth 2.0 (3LO) requires
//! the `api.atlassian.com` audience and uses space-delimited scopes.

const common = @import("common");
const oauth2 = @import("../oauth2.zig");

pub const PROVIDER = common.PROVIDER_JIRA;

const AUTHORIZE_ENDPOINT = "https://auth.atlassian.com/authorize";
const TOKEN_ENDPOINT = common.JIRA_TOKEN_ENDPOINT;
const SCOPES = "read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request offline_access";
const AUTHORIZE_EXTRA_QUERY = "audience=api.atlassian.com&prompt=consent";
const STATE_DOMAIN_PREFIX = "jira:v1:";
const STATE_NONCE_PREFIX = "connect:jira:nonce:";

pub const ACCESSIBLE_RESOURCES_ENDPOINT = "https://api.atlassian.com/oauth/token/accessible-resources";

pub const SPEC = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = AUTHORIZE_ENDPOINT,
    .token_endpoint = TOKEN_ENDPOINT,
    .scopes = SCOPES,
    .authorize_extra_query = AUTHORIZE_EXTRA_QUERY,
    .state = .{ .domain_prefix = STATE_DOMAIN_PREFIX, .nonce_prefix = STATE_NONCE_PREFIX },
};

test "jira OAuth requests issue and service desk reply scopes" {
    try @import("std").testing.expectEqualStrings(
        "read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request offline_access",
        SPEC.scopes,
    );
}
