//! GitHub status hook — the provider delta the generic status handler
//! (`connectors/status.zig`) dispatches to. "connected" iff the
//! `fleet:github` vault handle carries an installation id; never fabricates
//! a connected state (reconnect_required is surfaced by mint failures at use
//! time, not here).

const std = @import("std");
const hx_mod = @import("../../hx.zig");

const FIELD_INSTALLATION_ID = "installation_id";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";

/// Registry `respond_status` hook: `handle` is the parsed `fleet:github`
/// vault object (null = missing/unreadable). Owns the full response body.
pub fn respondStatus(hx: hx_mod.Hx, handle: ?std.json.ObjectMap) void {
    const connected = if (handle) |obj| obj.get(FIELD_INSTALLATION_ID) != null else false;
    hx.ok(.ok, .{ .status = if (connected) STATUS_CONNECTED else STATUS_NOT_CONNECTED });
}
