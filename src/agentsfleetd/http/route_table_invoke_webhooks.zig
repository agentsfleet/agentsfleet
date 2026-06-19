//! Webhook invokes split out of route_table_invoke.zig to keep that file
//! ≤ 350 lines per RULE FLL.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const webhooks = @import("handlers/webhooks/agent.zig");
const approval = @import("handlers/webhooks/approval.zig");
const grant_approval = @import("handlers/webhooks/grant_approval.zig");
const github_webhook_h = @import("handlers/webhooks/github.zig");
const clerk_webhook_h = @import("handlers/auth/identity_events_clerk.zig");

const Hx = hx_mod.Hx;

pub fn invokeReceiveWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    webhooks.innerReceiveWebhook(hx.*, req, route.receive_webhook);
}

pub fn invokeReceiveSvixWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    webhooks.innerReceiveSvixWebhook(hx.*, req, route.receive_svix_webhook);
}

// Clerk user.created auth-plane event. No middleware — handler verifies
// Svix signature inline against env CLERK_WEBHOOK_SECRET. Fn name kept as
// `invokeClerkWebhook` (path-rename only; PUB GATE intent is "don't grow
// the pub surface" and a rename is symmetric).
pub fn invokeClerkWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    clerk_webhook_h.innerClerkWebhook(hx.*, req);
}

pub fn invokeApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    approval.innerApprovalCallback(hx.*, req, route.approval_webhook);
}

pub fn invokeGrantApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    grant_approval.innerGrantApproval(hx.*, req, route.grant_approval_webhook);
}

pub fn invokeGithubWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    github_webhook_h.innerInvokeGithubWebhook(hx.*, req, route.github_webhook);
}
