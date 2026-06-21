//! Runner control-plane route matchers — extracted from route_matchers.zig
//! (RULE FLL) and re-exported there, mirroring route_matchers_fleet_bundles.zig.
//! Each takes `p: anytype` (a duck-typed `Path` exposing `.segs`, `.eq`,
//! `.param`) so this sibling needs no `Path` import (no cycle with the parent).

const runner_protocol = @import("contract").protocol;

const S_RUNNERS = "runners";
const S_ME = "me";
const S_LEASES = "leases";
const S_MEMORY = "memory";
const S_BUNDLES = "bundles";

/// Match `/runners/me/leases/{lease_id}/activity`. `me` is the self-plane
/// segment; identity is the Bearer token. The `activity` suffix is single-sourced
/// from the wire contract (RULE UFS).
pub fn matchRunnerLeaseActivity(p: anytype) ?[]const u8 {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_RUNNERS) or !p.eq(1, S_ME) or !p.eq(2, S_LEASES)) return null;
    if (!p.eq(4, runner_protocol.RUNNER_LEASE_ACTIVITY_SUFFIX)) return null;
    return p.param(3);
}

/// `GET|POST /v1/runners/me/memory/{fleet_id}` — runner-plane memory hydrate +
/// capture. 4 segments after the v1 strip; the fleet is the leaf. The method
/// (GET hydrate vs POST capture) is disambiguated by the router, not here.
pub fn matchRunnerMemory(p: anytype) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_RUNNERS) or !p.eq(1, S_ME) or !p.eq(2, S_MEMORY)) return null;
    return p.param(3);
}

/// `GET /v1/runners/me/bundles/{content_hash}` — runner-plane bundle snapshot
/// download. 4 segments after the v1 strip; the content hash is the leaf. The
/// daemon rebuilds the object-storage key from the hash (the key carries slashes
/// and so cannot be a single path param — the hash can).
pub fn matchRunnerBundles(p: anytype) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_RUNNERS) or !p.eq(1, S_ME) or !p.eq(2, S_BUNDLES)) return null;
    return p.param(3);
}

/// `POST /v1/runners/me/leases/{lease_id}/renew` — same shape as activity, keyed
/// on the `renew` suffix segment (single-sourced from the wire contract).
pub fn matchRunnerLeaseRenew(p: anytype) ?[]const u8 {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_RUNNERS) or !p.eq(1, S_ME) or !p.eq(2, S_LEASES)) return null;
    if (!p.eq(4, runner_protocol.RUNNER_LEASE_RENEW_SUFFIX)) return null;
    return p.param(3);
}
