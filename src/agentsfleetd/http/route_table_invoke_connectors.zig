//! Connector invokes — kept in a sibling file so route_table_invoke.zig stays
//! under the file-length budget (RULE FLL). One generic trio serves every
//! provider in the connector registry (connect/status carry the workspace +
//! provider captures; the callback reads its signed state from the query, so
//! its only capture is the provider). Slack's events ingress stays bespoke.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const connect_h = @import("handlers/connectors/connect.zig");
const callback_h = @import("handlers/connectors/callback.zig");
const status_h = @import("handlers/connectors/status.zig");
const slack_events_h = @import("handlers/connectors/slack/events.zig");

const Hx = hx_mod.Hx;

pub fn invokeConnectorConnect(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .POST)) return;
    connect_h.innerConnect(hx.*, req, route.connector_connect);
}

pub fn invokeConnectorStatus(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    status_h.innerStatus(hx.*, route.connector_status);
}

pub fn invokeConnectorCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    callback_h.innerCallback(hx.*, req, route.connector_callback);
}

pub fn invokeSlackEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .POST)) return;
    _ = route;
    slack_events_h.innerSlackEvents(hx.*, req);
}
