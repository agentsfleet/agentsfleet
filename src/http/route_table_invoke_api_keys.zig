//! Invoke dispatchers for /v1/api-keys — split out of route_table_invoke.zig
//! to keep that file under the 350-line RULE FLL cap.

const httpz = @import("httpz");
const router = @import("router.zig");
const hx_mod = @import("handlers/hx.zig");
const common = @import("handlers/common.zig");
const api_keys_h = @import("handlers/api_keys.zig");

const Hx = hx_mod.Hx;

pub fn invokeTenantApiKeys(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    switch (req.method) {
        .POST => api_keys_h.innerCreateApiKey(hx.*, req),
        .GET => api_keys_h.innerListApiKeys(hx.*, req),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeTenantApiKeyById(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const key_id = route.tenant_api_key_by_id;
    switch (req.method) {
        .PATCH => api_keys_h.innerPatchApiKey(hx.*, req, key_id),
        .DELETE => api_keys_h.innerDeleteApiKey(hx.*, key_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}
