//! GitHub connector invokes — kept in a sibling file so route_table_invoke.zig
//! stays under the file-length budget (RULE FLL). connect/status carry the
//! workspace id; the callback reads its signed state from the query (no path
//! param), so it ignores `route`.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const connect_h = @import("handlers/connectors/github/connect.zig");
const callback_h = @import("handlers/connectors/github/callback.zig");
const status_h = @import("handlers/connectors/github/status.zig");

const Hx = hx_mod.Hx;

pub fn invokeConnectGithub(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    connect_h.innerConnectGithub(hx.*, route.connect_github);
}

pub fn invokeGithubConnectorStatus(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    status_h.innerGithubStatus(hx.*, route.github_connector_status);
}

pub fn invokeGithubCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    _ = route;
    callback_h.innerGithubCallback(hx.*, req);
}
