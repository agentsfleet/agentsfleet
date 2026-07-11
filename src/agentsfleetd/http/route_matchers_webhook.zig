// Webhook route matchers — split out of route_matchers.zig to keep that file
// within the 350-line limit (RULE FLL). Operates on the same canonical `Path`
// view; the webhook-reserved segments live here as private predicates so the
// webhook matchers stay mutually exclusive with the approval / svix families.

const Path = @import("route_matchers.zig").Path;
const testing = @import("std").testing;

const S_WEBHOOKS = "webhooks";
const S_INGRESS = "ingress";
const RESERVED_SVIX = "svix";
const RESERVED_CLERK = "clerk";
const RESERVED_APPROVAL = "approval";
const RESERVED_GRANT_APPROVAL = "grant-approval";

pub fn matchWebhookAction(p: Path, action: []const u8) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_WEBHOOKS)) return null;
    if (!p.eq(2, action)) return null;
    if (p.eq(1, RESERVED_SVIX) or p.eq(1, RESERVED_CLERK)) return null;
    if (p.eq(1, RESERVED_APPROVAL) or p.eq(1, RESERVED_GRANT_APPROVAL)) return null;
    return p.param(1);
}

pub fn matchSvixWebhook(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_WEBHOOKS) or !p.eq(1, RESERVED_SVIX)) return null;
    return p.param(2);
}

/// Match `/webhooks/{fleet_id}` (HMAC-only). The 3-segment
/// `/webhooks/{fleet_id}/{action}` form is matched per-action by
/// `matchWebhookAction` (approval, grant-approval, github, …); the legacy
/// URL-embedded-secret form has been removed.
pub fn matchWebhook(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, S_WEBHOOKS)) return null;
    if (p.eq(1, RESERVED_SVIX) or p.eq(1, RESERVED_CLERK)) return null;
    if (p.eq(1, RESERVED_APPROVAL) or p.eq(1, RESERVED_GRANT_APPROVAL)) return null;
    return p.param(1);
}

/// Match `/ingress/{provider}`. Provider support is resolved by the verifier
/// registry in the handler, so the router captures any non-empty identifier.
pub fn matchIngress(p: Path) ?[]const u8 {
    if (p.segs.len != 2 or !p.eq(0, S_INGRESS)) return null;
    return p.param(1);
}

test "App ingress matcher captures one provider segment" {
    const provider = matchIngress(.{ .segs = &.{ "ingress", "github" } }) orelse return error.TestExpectedMatch;
    try testing.expectEqualStrings("github", provider);
    try testing.expect(matchIngress(.{ .segs = &.{"ingress"} }) == null);
    try testing.expect(matchIngress(.{ .segs = &.{ "ingress", "github", "extra" } }) == null);
}
