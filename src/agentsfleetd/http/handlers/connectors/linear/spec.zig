//! Linear OAuth connector descriptor. Linear uses comma-delimited scopes and
//! returns refresh-token pairs for authorization-code OAuth apps.

const common = @import("common");
const oauth2 = @import("../oauth2.zig");

pub const PROVIDER = common.PROVIDER_LINEAR;

const AUTHORIZE_ENDPOINT = "https://linear.app/oauth/authorize";
const TOKEN_ENDPOINT = common.LINEAR_TOKEN_ENDPOINT;
const SCOPES = "read,comments:create";
const STATE_DOMAIN_PREFIX = "linear:v1:";
const STATE_NONCE_PREFIX = "connect:linear:nonce:";

pub const SPEC = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = AUTHORIZE_ENDPOINT,
    .token_endpoint = TOKEN_ENDPOINT,
    .scopes = SCOPES,
    .state = .{ .domain_prefix = STATE_DOMAIN_PREFIX, .nonce_prefix = STATE_NONCE_PREFIX },
};

test "linear OAuth requests read and targeted comment reply scopes" {
    try @import("std").testing.expectEqualStrings("read,comments:create", SPEC.scopes);
}
