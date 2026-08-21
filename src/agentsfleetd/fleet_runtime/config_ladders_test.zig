//! Allocation-failure proofs for the config helpers' remaining `errdefer`
//! ladders — the rungs the success-path tests in `config_helpers_test.zig`
//! never touch.
//!
//! A ladder is only proven by the fixture that populates every rung it guards.
//! An absent optional leaves its rung holding null, which frees nothing: the
//! test passes, the rung was never exercised, and a missing `free` there stays
//! invisible until the daemon parses enough fleet configs to show it.
//!
//! Separate from `config_helpers_test.zig` because that file is already past
//! the 350-line limit; extending it would deepen the violation rather than
//! carry these proofs.

const std = @import("std");

const config_helpers = @import("config_helpers.zig");
const config_parser = @import("config_parser.zig");
const config_types = @import("config_types.zig");

/// Every optional populated: schedule, timezone and message each own a
/// separate allocation, so the two rungs above the message dupe are holding
/// real memory when a later allocation fails.
const PROOF_CRON_TRIGGER =
    \\{"type":"cron","schedule":"0 3 * * *","timezone":"Asia/Kolkata",
    \\ "message":"nightly sweep"}
;

/// `signature` present with header, prefix AND ts_header set. Without
/// ts_header nothing allocates after the prefix rung, so the prefix rung has
/// no failure to unwind through and line stays unproven.
const PROOF_SIGNED_WEBHOOK_TRIGGER =
    \\{"type":"webhook","source":"github","events":["push"],
    \\ "signature":{"secret_ref":"vault://wh","header":"X-Hub-Signature-256",
    \\ "prefix":"sha256=","ts_header":"X-Hub-Timestamp"}}
;

/// Both string arrays populated: a failure while duping `read_post_paths`
/// must unwind through the `allow` rung, and an `allow` list that is empty
/// makes that rung a no-op.
const PROOF_NETWORK =
    \\{"allow":["api.github.com","api.linear.app"],"read_only":true,
    \\ "read_post_paths":["/graphql","/v1/query"]}
;

fn parseTriggerUnderAllocator(alloc: std.mem.Allocator, src: []const u8) !void {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, src, .{});
    defer parsed.deinit();
    const trigger = try config_helpers.parseFleetTrigger(alloc, parsed.value.object);
    config_types.freeFleetTrigger(alloc, trigger);
}

fn parseCronTriggerUnderAllocator(alloc: std.mem.Allocator) !void {
    try parseTriggerUnderAllocator(alloc, PROOF_CRON_TRIGGER);
}

fn parseSignedWebhookUnderAllocator(alloc: std.mem.Allocator) !void {
    try parseTriggerUnderAllocator(alloc, PROOF_SIGNED_WEBHOOK_TRIGGER);
}

fn parseNetworkUnderAllocator(alloc: std.mem.Allocator) !void {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, PROOF_NETWORK, .{});
    defer parsed.deinit();
    const net = try config_helpers.parseFleetNetwork(alloc, parsed.value.object);
    config_types.freeStringSlice(alloc, net.allow);
    config_types.freeStringSlice(alloc, net.read_post_paths);
}

test "test_cron_trigger_parse_unwinds_without_leaking" {
    // The cron arm owns three strings behind two rungs. `timezone` and
    // `message` each take one of two paths — the caller's value or a dupe of
    // the default — and both paths allocate, so the rung must free either.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, parseCronTriggerUnderAllocator, .{});
}

test "test_signed_webhook_trigger_parse_unwinds_without_leaking" {
    // The signature block owns four strings, three of them behind rungs, and
    // it nests inside the webhook trigger's own four-rung ladder. A failure
    // anywhere in the inner block has to unwind both.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, parseSignedWebhookUnderAllocator, .{});
}

test "test_fleet_network_parse_unwinds_without_leaking" {
    // Two slices of slices. The `allow` rung must free the entries before the
    // backing array; freeing only the array leaks every string in it, which
    // the success path cannot show because nothing frees them there either.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, parseNetworkUnderAllocator, .{});
}

/// The whole-config ladder: nine rungs in `parseConfig`, of which the `network`
/// and `repository_binding` ones had never unwound. Both need their optional
/// block PRESENT to hold anything, and both need an allocation after them to
/// unwind through — `skill` is the one that provides it.
const PROOF_FULL_CONFIG =
    \\{"name":"ladder-proof","x-agentsfleet":{
    \\ "triggers":[{"type":"cron","schedule":"0 3 * * *","timezone":"Asia/Kolkata"}],
    \\ "tools":["agentmail"],"credentials":["agentmail_api_key"],
    \\ "network":{"allow":["api.agentmail.to"],"read_post_paths":["/graphql"]},
    \\ "budget":{"daily_dollars":1.0},
    \\ "repositories":["acme/one","acme/two"],"repository_access":"write",
    \\ "repository_base":"main",
    \\ "skill":"ops/triage"}}
;

fn parseFullConfigUnderAllocator(alloc: std.mem.Allocator) !void {
    var cfg = try config_parser.parseFleetConfig(alloc, PROOF_FULL_CONFIG);
    cfg.deinit(alloc);
}

test "test_full_config_parse_unwinds_without_leaking" {
    // Nine rungs, two of them freeing slices of slices and one freeing a
    // struct's two string arrays. A config missing `network` or the repository
    // trio leaves those rungs holding null, which is why the parser's existing
    // tests never exercised them.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, parseFullConfigUnderAllocator, .{});
}
