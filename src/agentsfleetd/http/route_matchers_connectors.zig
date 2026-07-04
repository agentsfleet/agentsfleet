// Connector route matchers — split out of route_matchers.zig to keep that
// file within the 350-line limit (RULE FLL). Operates on the same canonical
// `Path` view. One generic `{provider}` trio serves every provider in the
// connector registry (`handlers/connectors/registry.zig` — the invoke layer
// resolves the captured segment and 404s unknown ids with a body naming
// them), under the platform namespace: `/v1/workspaces/{ws}/connectors/*`
// authed + `/v1/connectors/{provider}/callback` state-authed. Slack's events
// ingress keeps its bespoke matcher — inbound event surfaces are per-provider
// by nature.

const Path = @import("route_matchers.zig").Path;
const common = @import("common");

const S_WORKSPACES = "workspaces";
const S_CONNECTORS = "connectors";
const S_CONNECT = "connect";
const S_CALLBACK = "callback";
const S_EVENTS = "events";

/// Captures of one workspace-scoped `{provider}` connector route.
pub const WorkspaceConnectorRoute = struct {
    workspace_id: []const u8,
    provider: []const u8,
};

/// GET /v1/workspaces/{ws}/connectors/{provider} — connector status.
pub fn matchWorkspaceConnector(p: Path) ?WorkspaceConnectorRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS)) return null;
    const ws = p.param(1) orelse return null;
    const provider = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .provider = provider };
}

/// POST /v1/workspaces/{ws}/connectors/{provider}/connect — start the flow.
pub fn matchWorkspaceConnectorConnect(p: Path) ?WorkspaceConnectorRoute {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS) or !p.eq(4, S_CONNECT)) return null;
    const ws = p.param(1) orelse return null;
    const provider = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .provider = provider };
}

/// GET /v1/workspaces/{ws}/connectors — the registry-driven catalog, scoped to
/// the workspace (its `connected` flags are per-workspace). The collection whose
/// items are the `/v1/workspaces/{ws}/connectors/{provider}` status routes.
pub fn matchWorkspaceConnectorCatalog(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_CONNECTORS)) return null;
    return p.param(1);
}

/// GET /v1/connectors/{provider}/callback — Bearer-less; the workspace comes
/// from the signed state.
pub fn matchConnectorCallback(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_CONNECTORS) or !p.eq(2, S_CALLBACK)) return null;
    return p.param(1);
}

/// POST /v1/connectors/slack/events — Bearer-less; authenticated by the Slack
/// v0 request signature in-handler. Bespoke by design (each provider's inbound
/// event surface has its own shape); the `events` segment + POST verb
/// disambiguate it from the generic `callback` matcher above.
pub fn matchSlackEvents(p: Path) bool {
    if (p.segs.len != 3) return false;
    return p.eq(0, S_CONNECTORS) and p.eq(1, common.PROVIDER_SLACK) and p.eq(2, S_EVENTS);
}
