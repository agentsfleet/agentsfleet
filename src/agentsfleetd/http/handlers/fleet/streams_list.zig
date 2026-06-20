//! GET /v1/fleets/streams — platform-admin listing of this instance's live
//! SSE streams (operator diagnostics for the stream cap and shutdown drain).
//!
//! Rows come straight from the StreamRegistry: workspace, fleet, and start
//! time per live stream. The registry's client fd never leaves the process.
//! Fleet-plane endpoint — like the rest of /v1/fleet it is deliberately
//! absent from the public OpenAPI document (customer surface only).

const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

const Hx = hx_mod.Hx;

const MSG_OUT_OF_MEMORY = "Out of memory";

pub fn innerListFleetStreams(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const rows = hx.ctx.stream_registry.listAlloc(hx.alloc) catch {
        common.internalOperationError(hx.res, MSG_OUT_OF_MEMORY, hx.req_id);
        return;
    };
    // List envelope per the REST guidelines and the fleet-plane sibling
    // (runners_list): items + total, plus the cap the count runs against.
    hx.ok(.ok, .{
        .items = rows,
        .total = rows.len,
        .max_streams = hx.ctx.sse_max_streams,
    });
}
