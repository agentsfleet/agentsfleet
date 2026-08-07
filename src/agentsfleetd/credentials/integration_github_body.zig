//! What the mint ASKS GitHub for: the installation-token request body, built
//! from the fleet's repository binding.
//!
//! The response-side half lives in `integration_github_reach.zig`. Splitting the
//! two apart is not only the file-length budget (RULE FLL) — they answer
//! different questions and fail for different reasons. This module decides what
//! a fleet may ask for and refuses a binding it cannot express; that one decides
//! whether the token GitHub actually returned matches what was asked.

const std = @import("std");
const integration = @import("integration.zig");

const MintCtx = integration.MintCtx;

// Installation-token request body fields + permission values (RULE UFS — shared
// verbatim with the bundle frontmatter and the mint-body tests).
const REQ_FIELD_REPOSITORIES = "repositories";
const REQ_FIELD_PERMISSIONS = "permissions";
const PERM_CONTENTS = "contents";
const PERM_PULL_REQUESTS = "pull_requests";
const PERM_VALUE_READ = "read";
const PERM_VALUE_WRITE = "write";

/// Build the installation-token request body from the fleet's repository binding.
/// Returns null when the fleet declared none, so the caller fails closed.
///
/// The body is what bounds the token. An empty body — the prior behaviour — asks
/// GitHub for the App installation's full permission set across every repository
/// it covers, valid for an hour. Naming `repositories` and `permissions` narrows
/// it to what the fleet declared, and a read-scoped fleet never receives a
/// `pull_requests` key at all rather than receiving it set to "read".
pub fn buildTokenRequestBody(ctx: MintCtx) !?[]u8 {
    const binding = ctx.repository_binding orelse return null;
    if (binding.repositories.len == 0) return null;

    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(ctx.alloc);

    try out.appendSlice(ctx.alloc, "{\"" ++ REQ_FIELD_REPOSITORIES ++ "\":[");
    for (binding.repositories, 0..) |qualified, i| {
        const name = bareRepositoryName(qualified);
        if (!isSafeRepositoryName(name)) {
            // Refuse, never escape. `errdefer` does not fire on a null return,
            // so the partially-built body is released explicitly here.
            out.deinit(ctx.alloc);
            return null;
        }
        if (i > 0) try out.append(ctx.alloc, ',');
        try out.append(ctx.alloc, '"');
        try out.appendSlice(ctx.alloc, name);
        try out.append(ctx.alloc, '"');
    }
    try out.appendSlice(ctx.alloc, "],\"" ++ REQ_FIELD_PERMISSIONS ++ "\":{\"" ++ PERM_CONTENTS ++ "\":\"");
    try out.appendSlice(ctx.alloc, switch (binding.access) {
        .read => PERM_VALUE_READ,
        .write => PERM_VALUE_WRITE,
    });
    try out.append(ctx.alloc, '"');
    // pull_requests is granted ONLY at write. Its absence is the read scope —
    // opening a Pull Request with this token then fails at the vendor.
    if (binding.access == .write) {
        try out.appendSlice(ctx.alloc, ",\"" ++ PERM_PULL_REQUESTS ++ "\":\"" ++ PERM_VALUE_WRITE ++ "\"");
    }
    try out.appendSlice(ctx.alloc, "}}");
    return try out.toOwnedSlice(ctx.alloc);
}

/// GitHub scopes an installation token by repository NAME, not `owner/repo`: the
/// installation already fixes the owner, so a slashed value matches no repository
/// and silently yields a token scoped to nothing. The binding keeps the qualified
/// spelling — that is what a fleet author writes and what the repository-fetch
/// validation compares against — so the owner is stripped here, at the wire edge.
///
/// Stripping it is also why `integration_github_reach.zig` exists: the owner a
/// fleet declared never reaches GitHub, so only the response can say whether the
/// token landed on the repository the fleet actually named.
fn bareRepositoryName(qualified: []const u8) []const u8 {
    const slash = std.mem.lastIndexOfScalar(u8, qualified, '/') orelse return qualified;
    return qualified[slash + 1 ..];
}

/// GitHub repository names carry only alphanumerics, `-`, `_`, and `.`. Anything
/// else is not a repository name, so the mint REFUSES rather than escaping it
/// into the body — a value needing escaping could never have matched a real
/// repository, and refusing keeps this builder free of an escaping path.
fn isSafeRepositoryName(name: []const u8) bool {
    if (name.len == 0) return false;
    for (name) |c| {
        if (!std.ascii.isAlphanumeric(c) and c != '-' and c != '_' and c != '.') return false;
    }
    return true;
}
