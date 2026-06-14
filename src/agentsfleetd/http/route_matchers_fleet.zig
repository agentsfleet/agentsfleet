//! Fleet operator-plane route matchers.

const S_EVENTS = "events";
const S_FLEET = "fleet";
const S_RUNNERS = "runners";

/// Match `/fleet/runners/{runner_id}` after the `/v1` prefix is stripped.
pub fn matchFleetRunner(p: anytype) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_FLEET) or !p.eq(1, S_RUNNERS)) return null;
    return p.param(2);
}

/// Match `/fleet/runners/{runner_id}/events` after the `/v1` prefix is stripped.
pub fn matchFleetRunnerEvents(p: anytype) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_FLEET) or !p.eq(1, S_RUNNERS) or !p.eq(3, S_EVENTS)) return null;
    return p.param(2);
}
