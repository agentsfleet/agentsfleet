// Webhook route matchers — split out of route_matchers.zig to keep that file
// within the 350-line limit (RULE FLL). Operates on the same canonical `Path`
// view; the webhook-reserved segments live here as private predicates so the
// webhook matchers stay mutually exclusive with the approval / svix families.

const Path = @import("route_matchers.zig").Path;

const S_WEBHOOKS = "webhooks";
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
/// URL-embedded-secret variant has been removed.
pub fn matchWebhook(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, S_WEBHOOKS)) return null;
    if (p.eq(1, RESERVED_SVIX) or p.eq(1, RESERVED_CLERK)) return null;
    if (p.eq(1, RESERVED_APPROVAL) or p.eq(1, RESERVED_GRANT_APPROVAL)) return null;
    return p.param(1);
}
