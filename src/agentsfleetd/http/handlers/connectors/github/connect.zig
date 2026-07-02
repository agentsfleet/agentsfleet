//! GitHub connect hook — the provider delta the generic connect handler
//! (`connectors/connect.zig`) dispatches to for the `app_install` archetype:
//! build the GitHub App install URL the browser is redirected to. The App
//! slug is platform config resolved at boot; absent it, connect degrades
//! closed rather than minting a state that can never complete. No token is
//! created or stored here — the round-trip finishes at the callback, which
//! writes the vault handle the credential broker mints from.

const std = @import("std");
const hx_mod = @import("../../hx.zig");

const INSTALL_URL_FMT = "https://github.com/apps/{s}/installations/new?state={s}";

pub const BuildError = error{ NotConfigured, OutOfMemory };

/// Registry `build_install_url` hook. `st` is the minted single-use state
/// (base64url + '.' + hex — URL-safe, rides the query unescaped). Caller owns
/// the returned URL.
pub fn buildInstallUrl(hx: hx_mod.Hx, st: []const u8) BuildError![]const u8 {
    const slug = hx.ctx.github_app_slug orelse return BuildError.NotConfigured;
    return std.fmt.allocPrint(hx.alloc, INSTALL_URL_FMT, .{ slug, st });
}
