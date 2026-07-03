//! Linear OAuth connector descriptor. Linear's current OAuth docs use comma
//! scopes and return refresh-token pairs for OAuth apps.

const common = @import("common");
const oauth2 = @import("../oauth2.zig");

pub const PROVIDER = common.PROVIDER_LINEAR;

const AUTHORIZE_ENDPOINT = "https://linear.app/oauth/authorize";
const TOKEN_ENDPOINT = common.LINEAR_TOKEN_ENDPOINT;
const SCOPES = "read";
const STATE_DOMAIN_PREFIX = "linear:v1:";
const STATE_NONCE_PREFIX = "connect:linear:nonce:";

pub const SPEC = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = AUTHORIZE_ENDPOINT,
    .token_endpoint = TOKEN_ENDPOINT,
    .scopes = SCOPES,
    .state = .{ .domain_prefix = STATE_DOMAIN_PREFIX, .nonce_prefix = STATE_NONCE_PREFIX },
};
