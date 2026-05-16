// Actor-opacity invariant tests.
//
// The platform's worker contract: `actor` is a free-form `[]const u8`
// label that the SKILL.md prose interprets at execution time. The
// worker (event_loop + writepath) NEVER branches on the actor's
// content, prefix, or shape — it carries the bytes through the wire
// path (XADD), pins them into a SQL parameter, and surfaces them
// verbatim to consumers (history, SSE). This decoupling is what lets
// a user invent a brand-new actor prefix (`actor=ci:dispatch:job-42`)
// without any platform-side change.
//
// These tests pin the invariant at four surfaces:
//
//   (1) Envelope encoding — encodeForXAdd produces the same argv
//       SHAPE for every actor variant; only the index-3 value differs.
//   (2) Continuation wrapping — buildContinuationActor preserves the
//       source actor body verbatim and never inspects its prefix
//       beyond the idempotency check.
//   (3) EventType orthogonality — every (actor, event_type) pair is
//       a legal envelope; no cross-field validation gates these.
//   (4) Writepath SQL — the INSERT into core.zombie_events binds
//       event.actor as a single positional parameter ($4) without
//       any LIKE / CASE / position()-style pattern matching.
//
// A drift in any of these = the platform started caring about actor
// content; M68 Invariant #3 ("actor field is prompt-side load-bearing,
// platform-side opaque") fails until restored.

const std = @import("std");
const EventEnvelope = @import("event_envelope.zig");

// Five canonical variants — first three are the four-actor envelope
// from the architecture doc (steer / webhook / cron / continuation),
// plus a fifth "user-invented prefix" variant proving the contract
// holds for prefixes the platform has never heard of.
const ACTOR_VARIANTS = [_][]const u8{
    "steer:howdy",
    "webhook:github",
    "cron:0",
    "continuation:webhook:github",
    "future:ci:dispatch:job-42",
};

const SAMPLE_REQUEST = "{\"message\":\"hello\"}";

fn makeEnvelope(actor: []const u8, ev_type: EventEnvelope.EventType) EventEnvelope {
    return .{
        .event_id = "1729874000000-0",
        .zombie_id = "zb-1",
        .workspace_id = "ws-1",
        .actor = actor,
        .event_type = ev_type,
        .request_json = SAMPLE_REQUEST,
        .created_at = 1745568000000,
    };
}

// Surface (1) — XADD argv shape opacity.
test "actor opacity: encodeForXAdd produces identical argv shape for every prefix" {
    const alloc = std.testing.allocator;

    // Baseline argv from the first variant; every subsequent variant
    // must match it slot-for-slot except at index 3 (the actor value).
    const baseline_env = makeEnvelope(ACTOR_VARIANTS[0], .chat);
    const baseline = try baseline_env.encodeForXAdd(alloc);
    defer EventEnvelope.freeXAddArgv(alloc, baseline);
    try std.testing.expectEqual(@as(usize, 10), baseline.len);

    for (ACTOR_VARIANTS[1..]) |actor| {
        const env = makeEnvelope(actor, .chat);
        const argv = try env.encodeForXAdd(alloc);
        defer EventEnvelope.freeXAddArgv(alloc, argv);

        try std.testing.expectEqual(baseline.len, argv.len);
        for (baseline, argv, 0..) |b, a, i| {
            if (i == 3) {
                // index 3 is the actor value; drives the variance.
                try std.testing.expectEqualStrings(actor, a);
            } else {
                // every other slot — field names and the other values
                // must be byte-identical across variants.
                try std.testing.expectEqualStrings(b, a);
            }
        }
    }
}

// Surface (1, negative side) — the actor VALUE must round-trip
// verbatim, including prefixes the platform has never seen.
test "actor opacity: encodeForXAdd preserves actor bytes verbatim for unknown prefix" {
    const alloc = std.testing.allocator;
    const exotic = "deepfuture::nested:colons:42";
    const env = makeEnvelope(exotic, .webhook);
    const argv = try env.encodeForXAdd(alloc);
    defer EventEnvelope.freeXAddArgv(alloc, argv);
    try std.testing.expectEqualStrings(exotic, argv[3]);
}

// Surface (2) — buildContinuationActor preserves arbitrary actor
// bodies. Wraps with the `continuation:` prefix once and only once;
// the body that follows is byte-for-byte the source actor.
test "actor opacity: buildContinuationActor wraps every variant byte-verbatim" {
    const alloc = std.testing.allocator;
    for (ACTOR_VARIANTS) |actor| {
        const out = try EventEnvelope.buildContinuationActor(alloc, actor);
        defer alloc.free(out);

        if (std.mem.startsWith(u8, actor, EventEnvelope.continuation_actor_prefix)) {
            // Already-continuation → idempotent, no double-wrap.
            try std.testing.expectEqualStrings(actor, out);
        } else {
            // Fresh actor → exactly one `continuation:` prefix + the
            // original bytes appended verbatim.
            try std.testing.expect(std.mem.startsWith(u8, out, EventEnvelope.continuation_actor_prefix));
            const suffix = out[EventEnvelope.continuation_actor_prefix.len..];
            try std.testing.expectEqualStrings(actor, suffix);
        }
    }
}

// Surface (3) — EventType is orthogonal to actor; cross-product is
// the legal envelope set. A platform that secretly required
// `actor=steer:*` to pair with `event_type=chat` would fail here.
test "actor opacity: every (actor, event_type) pair builds a valid envelope" {
    const alloc = std.testing.allocator;
    const TYPES = [_]EventEnvelope.EventType{ .chat, .webhook, .cron, .continuation };

    for (ACTOR_VARIANTS) |actor| {
        for (TYPES) |ev_type| {
            const env = makeEnvelope(actor, ev_type);
            const argv = try env.encodeForXAdd(alloc);
            defer EventEnvelope.freeXAddArgv(alloc, argv);

            // type field reflects ev_type; actor field reflects actor.
            // Neither leaks into the other.
            try std.testing.expectEqualStrings(ev_type.toSlice(), argv[1]);
            try std.testing.expectEqualStrings(actor, argv[3]);
        }
    }
}

// Surface (4) — writepath SQL never pattern-matches the actor column.
// Reads event_loop_writepath_rows.zig at comptime via @embedFile (the
// file is a same-directory sibling, so the src/ boundary memory does
// not apply). Asserts the INSERT statement binds `actor` as a single
// positional parameter without any prefix-aware SQL operator.
//
// Drift signal: if a future commit adds `WHERE actor LIKE 'steer:%'`
// or `CASE WHEN actor SIMILAR TO ...` to writepath_rows.zig, this
// test fails and forces the spec-vs-code conversation. (Anti-patterns
// list — extend if a new actor-aware SQL form appears in PG docs.)
test "actor opacity: writepath SQL binds actor as a single positional parameter" {
    const writepath_src = @embedFile("event_loop_writepath_rows.zig");

    // The actor column must appear in the INSERT field list verbatim;
    // pins the contract that the column exists and is written.
    try std.testing.expect(std.mem.indexOf(u8, writepath_src, "  (zombie_id, event_id, workspace_id, actor, event_type,") != null);
    // The actor parameter must be bound at the $4 slot; pins the
    // positional binding and rules out an intermediate transformation.
    try std.testing.expect(std.mem.indexOf(u8, writepath_src, "VALUES ($1::uuid, $2, $3::uuid, $4, $5,") != null);

    // Anti-pattern grep — every form of actor-prefix-aware SQL the
    // platform must NOT contain. If any of these appear in writepath
    // rows, the opacity invariant has broken.
    const ANTI_PATTERNS = [_][]const u8{
        "actor LIKE",
        "actor like",
        "actor ~",
        "actor SIMILAR TO",
        "actor similar to",
        "CASE actor",
        "case actor",
        "WHEN actor",
        "when actor",
        "position(",
        "starts_with(actor",
        "left(actor",
    };
    for (ANTI_PATTERNS) |pat| {
        if (std.mem.indexOf(u8, writepath_src, pat) != null) {
            std.debug.print("actor-opacity violation: writepath SQL contains '{s}'\n", .{pat});
            try std.testing.expect(false);
        }
    }
}
