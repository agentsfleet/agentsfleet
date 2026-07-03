//! Zoho Desk OAuth connector descriptor. Endpoints/scopes mirror Zoho Desk's
//! current OAuth docs: Accounts auth/token endpoints and the minimal read
//! scopes needed to identify the connected organization.

const common = @import("common");
const oauth2 = @import("../oauth2.zig");

pub const PROVIDER = common.PROVIDER_ZOHO;

const AUTHORIZE_ENDPOINT = "https://accounts.zoho.com/oauth/v2/auth";
const TOKEN_ENDPOINT = "https://accounts.zoho.com/oauth/v2/token";
const SCOPES = "Desk.organization.READ,Desk.basic.READ";
const AUTHORIZE_EXTRA_QUERY = "access_type=offline&prompt=consent";
const STATE_DOMAIN_PREFIX = "zoho:v1:";
const STATE_NONCE_PREFIX = "connect:zoho:nonce:";

pub const SPEC = oauth2.Spec{
    .provider = PROVIDER,
    .authorize_endpoint = AUTHORIZE_ENDPOINT,
    .token_endpoint = TOKEN_ENDPOINT,
    .scopes = SCOPES,
    .authorize_extra_query = AUTHORIZE_EXTRA_QUERY,
    .state = .{ .domain_prefix = STATE_DOMAIN_PREFIX, .nonce_prefix = STATE_NONCE_PREFIX },
};
