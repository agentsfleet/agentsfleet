//! Body parsing + validation for the fleet PATCH (split from `patch.zig`
//! under the file-length cap; `patch.zig` owns the transaction and FSM).

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const create = @import("create.zig");

const Hx = hx_mod.Hx;

pub const PatchBody = struct {
    config_json: ?[]const u8 = null,
    status: ?[]const u8 = null,
    trigger_markdown: ?[]const u8 = null,
    source_markdown: ?[]const u8 = null,
};

pub fn parsePatchBody(hx: Hx, req: *httpz.Request) ?PatchBody {
    const body = req.body() orelse return .{};
    if (body.len == 0) return .{};
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

pub fn validateBody(hx: Hx, body: PatchBody) bool {
    // config_json and trigger_markdown both drive core.fleets.config_json;
    // sending both is ambiguous — reject at the door.
    if (body.config_json != null and body.trigger_markdown != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, "config_json and trigger_markdown are mutually exclusive");
        return false;
    }
    if (body.config_json) |cj| {
        if (cj.len == 0) {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_CONFIG_REQUIRED);
            return false;
        }
    }
    // Same 1..64KiB cap as create (`create.MAX_*_LEN`): an edit cannot smuggle an
    // oversized body past the create-time guard. Load-bearing now that the
    // `SKILL.md` body rides every lease — an unbounded source_markdown would
    // inflate every lease toward MAX_LEASE_BYTES and the model context.
    if (body.trigger_markdown) |tm| if (tm.len == 0 or tm.len > create.MAX_TRIGGER_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "trigger_markdown must be 1..64KiB");
        return false;
    };
    if (body.source_markdown) |sm| if (sm.len == 0 or sm.len > create.MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "source_markdown must be 1..64KiB");
        return false;
    };
    if (body.status) |s| {
        // Only operator-targetable states. `paused` is reserved for the
        // platform's anomaly gate and rejected here so callers can't use
        // PATCH to forge a system-halt provenance.
        const allowed =
            std.mem.eql(u8, s, fleet_config.FleetStatus.active.toSlice()) or
            std.mem.eql(u8, s, fleet_config.FleetStatus.stopped.toSlice()) or
            std.mem.eql(u8, s, fleet_config.FleetStatus.killed.toSlice());
        if (!allowed) {
            hx.fail(ec.ERR_INVALID_REQUEST, "status must be one of \"active\", \"stopped\", \"killed\"");
            return false;
        }
    }
    return true;
}
