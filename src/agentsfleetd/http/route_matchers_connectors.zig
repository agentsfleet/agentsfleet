// Connector OAuth route matchers — split out of route_matchers.zig to keep
// that file within the 350-line limit (RULE FLL). Operates on the same
// canonical `Path` view. Covers the GitHub App-install + Slack OAuth connect
// and their Bearer-less callback routes under the single platform namespace
// (`/v1/workspaces/{ws}/connectors/*` authed + `/v1/connectors/*/callback`
// state-authed). The connector-reserved segments live here as private
// predicates, mirroring route_matchers_webhook.zig.

const Path = @import("route_matchers.zig").Path;

const S_WORKSPACES = "workspaces";
const S_CONNECTORS = "connectors";
const S_GITHUB = "github";
const S_SLACK = "slack";
const S_CONNECT = "connect";
const S_CALLBACK = "callback";

/// GET /v1/workspaces/{ws}/connectors/github — connector status.
pub fn matchWorkspaceConnectorGithub(p: Path) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS) or !p.eq(3, S_GITHUB)) return null;
    return p.param(1);
}

/// POST /v1/workspaces/{ws}/connectors/github/connect — start the App install.
pub fn matchWorkspaceConnectorGithubConnect(p: Path) ?[]const u8 {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS) or !p.eq(3, S_GITHUB) or !p.eq(4, S_CONNECT)) return null;
    return p.param(1);
}

/// GET /v1/connectors/github/callback — Bearer-less; workspace comes from the signed state.
pub fn matchGithubConnectCallback(p: Path) bool {
    if (p.segs.len != 3) return false;
    return p.eq(0, S_CONNECTORS) and p.eq(1, S_GITHUB) and p.eq(2, S_CALLBACK);
}

/// POST /v1/workspaces/{ws}/connectors/slack/connect — start the OAuth flow.
pub fn matchWorkspaceConnectorSlackConnect(p: Path) ?[]const u8 {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS) or !p.eq(3, S_SLACK) or !p.eq(4, S_CONNECT)) return null;
    return p.param(1);
}

/// GET /v1/connectors/slack/callback — Bearer-less; workspace comes from the signed state.
pub fn matchSlackConnectCallback(p: Path) bool {
    if (p.segs.len != 3) return false;
    return p.eq(0, S_CONNECTORS) and p.eq(1, S_SLACK) and p.eq(2, S_CALLBACK);
}
