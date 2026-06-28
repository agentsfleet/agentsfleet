//! control_plane_client_mint.zig — the on-demand credential-mint verb (M102 §3).
//!
//! Split out of `control_plane_client.zig` (RULE FLL — that file sits at the line
//! cap) and re-exported there, so callers keep using `cp.mint(...)` unchanged.
//! Mirrors the daemon's `protocol_credentials.zig` split. The verb shares the
//! parent client's persistent connection pool + deadline watchdog via its pub
//! `post` primitive — no second HTTP client, no drift from the other verbs.

const std = @import("std");
const Allocator = std.mem.Allocator;
const protocol = @import("contract").protocol;
const LoopbackClient = @import("control_plane_client.zig");

/// Outcome of a `/credentials/mint` forward the parent frames back to the child.
pub const MintOutcome = union(enum) {
    /// 2xx — the short-lived workspace-scoped token (duped into the caller's
    /// alloc) + its epoch-ms expiry. Secret (VLT): the caller frames it straight
    /// to the child's stdin and never logs it; the caller frees `token`.
    minted: struct { token: []const u8, expires_at_ms: i64 },
    /// Any non-2xx (a typed `UZ-CRED-*` / `UZ-GH-*` envelope) or a transport/parse
    /// failure: the child fails the tool call closed. No token to free.
    rejected,
};

/// POST /v1/runners/me/credentials/mint → forward the sandboxed child's on-demand
/// mint ask to the daemon-side broker over the agt_r plane (`data_flow.md` §B).
/// `lease_id` binds the mint to the lease's workspace **server-side** (Invariant
/// 2): the child never names a workspace, so this verb does not accept one — a
/// prompt-injected child cannot mint for another tenant. Fail-closed by design:
/// every failure (transport, non-2xx typed envelope, malformed body) collapses to
/// `.rejected` so the tool call aborts rather than dispatching with a stale/blank
/// credential. On success `token` is duped into `alloc` (outlives `res.body`,
/// freed here) for the caller to frame to the child, then free.
pub fn mint(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    lease_id: []const u8,
    integration: []const u8,
    scope: ?[]const u8,
    deadline_ms: u31,
) MintOutcome {
    const body = std.json.Stringify.valueAlloc(alloc, protocol.MintCredentialRequest{
        .lease_id = lease_id,
        .integration = integration,
        .scope = scope,
    }, .{}) catch return .rejected;
    defer alloc.free(body);
    const res = self.post(alloc, protocol.PATH_RUNNER_CREDENTIALS_MINT, runner_token, body, deadline_ms) catch return .rejected;
    defer alloc.free(res.body);
    if (res.status < 200 or res.status >= 300) return .rejected;
    const parsed = std.json.parseFromSlice(protocol.MintCredentialResponse, alloc, res.body, .{}) catch return .rejected;
    defer parsed.deinit();
    const token = alloc.dupe(u8, parsed.value.token) catch return .rejected;
    return .{ .minted = .{ .token = token, .expires_at_ms = parsed.value.expires_at_ms } };
}
