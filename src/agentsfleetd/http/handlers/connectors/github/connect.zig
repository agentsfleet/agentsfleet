//! GitHub connect hook — the provider delta the generic connect handler
//! (`connectors/connect.zig`) dispatches to for the `app_install` archetype:
//! start with GitHub user authorization so an App installation that outlived
//! the agentsfleet datastore can be discovered. A user with no accessible
//! installation is sent through the App install URL from the callback.

const std = @import("std");
const pg = @import("pg");
const hx_mod = @import("../../hx.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const INSTALL_URL_FMT = "https://github.com/apps/{s}/installations/new?state={s}";

pub const BuildError = error{ NotConfigured, OutOfMemory };

/// Registry connection hook. Requiring both the App slug and OAuth client
/// credentials keeps the zero-install fallback and existing-install recovery
/// equally fail-closed.
pub fn buildConnectUrl(hx: hx_mod.Hx, conn: *pg.Conn, redirect_uri: []const u8, st: []const u8) BuildError![]const u8 {
    _ = hx.ctx.github_app_slug orelse return BuildError.NotConfigured;
    const creds = oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.PROVIDER) orelse return BuildError.NotConfigured;
    defer creds.deinit(hx.alloc);
    return oauth2.authorizeUrl(hx.alloc, spec.USER_AUTH, creds.client_id, redirect_uri, st) catch BuildError.OutOfMemory;
}

/// Registry `build_install_url` hook. `st` is the minted single-use state
/// (base64url + '.' + hex — URL-safe, rides the query unescaped). Caller owns
/// the returned URL.
pub fn buildInstallUrl(hx: hx_mod.Hx, st: []const u8) BuildError![]const u8 {
    const slug = hx.ctx.github_app_slug orelse return BuildError.NotConfigured;
    return std.fmt.allocPrint(hx.alloc, INSTALL_URL_FMT, .{ slug, st });
}
