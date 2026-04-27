// Zombie config value types.
//
// Pure data — no parsing, no I/O. Extracted from config.zig per M28_004.
// Destructors live here so they stay next to the type they free.

const std = @import("std");
const Allocator = std.mem.Allocator;
const config_gates = @import("config_gates.zig");

pub const ZombieConfigError = error{
    MissingRequiredField,
    InvalidTriggerType,
    InvalidTriggerSource,
    UnknownSkill,
    InvalidCredentialRef,
    InvalidBudget,
    InvalidSignatureConfig,
};

pub const ZombieStatus = enum {
    active,
    paused,
    stopped,
    killed,

    pub fn toSlice(self: ZombieStatus) []const u8 {
        return switch (self) {
            .active => "active",
            .paused => "paused",
            .stopped => "stopped",
            .killed => "killed",
        };
    }

    pub fn fromSlice(s: []const u8) ?ZombieStatus {
        if (std.mem.eql(u8, s, "active")) return .active;
        if (std.mem.eql(u8, s, "paused")) return .paused;
        if (std.mem.eql(u8, s, "stopped")) return .stopped;
        if (std.mem.eql(u8, s, "killed")) return .killed;
        return null;
    }

    pub fn isTerminal(self: ZombieStatus) bool {
        return self == .killed;
    }

    pub fn isRunnable(self: ZombieStatus) bool {
        return self == .active;
    }
};

pub const ZombieTriggerType = enum { webhook, cron, api, chain };

pub const MAX_SIGNATURE_HEADER_LEN: usize = 64;

pub const WebhookSignatureConfig = struct {
    header: []const u8,
    prefix: []const u8,
    ts_header: ?[]const u8 = null,
    secret_ref: []const u8,
};

/// Tagged union for trigger config. Each variant carries only the fields it needs,
/// making invalid states (e.g. webhook without source) unrepresentable.
pub const ZombieTrigger = union(ZombieTriggerType) {
    webhook: struct { source: []const u8, event: ?[]const u8, signature: ?WebhookSignatureConfig = null },
    cron: struct { schedule: []const u8 },
    api: void,
    chain: struct { source: []const u8 },
};

pub const ZombieBudget = struct {
    daily_dollars: f64,
    monthly_dollars: ?f64,
};

pub const ZombieNetwork = struct {
    allow: []const []const u8,
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const ZombieConfig = struct {
    name: []const u8,
    trigger: ZombieTrigger,
    skills: []const []const u8,
    credentials: []const []const u8,
    network: ?ZombieNetwork,
    budget: ZombieBudget,
    gates: ?config_gates.GatePolicy,
    // M2_002: ClaHub skill reference (e.g. "clawhub://queen/lead-hunter@1.0.1")
    // Resolution deferred — stored but not fetched.
    skill: ?[]const u8,
    // M2_002: Downstream zombies to chain events to.
    chain: []const []const u8,

    pub fn deinit(self: *const ZombieConfig, alloc: Allocator) void {
        alloc.free(self.name);
        freeZombieTrigger(alloc, self.trigger);
        freeStringSlice(alloc, self.skills);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| freeStringSlice(alloc, net.allow);
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
        if (self.skill) |s| alloc.free(s);
        freeStringSlice(alloc, self.chain);
    }
};

// Guards against silent field drift: if a field is added to ZombieConfig
// without updating deinit(), @sizeOf changes and this assert fails at compile.
// 288 bytes on 64-bit: 9 pointer/slice fields + trigger union + budget + gates optional.
comptime {
    std.debug.assert(@sizeOf(ZombieConfig) == 288);
}

pub fn freeStringSlice(alloc: Allocator, slice: []const []const u8) void {
    for (slice) |s| alloc.free(s);
    alloc.free(slice);
}

pub fn freeZombieTrigger(alloc: Allocator, t: ZombieTrigger) void {
    switch (t) {
        .webhook => |w| {
            alloc.free(w.source);
            if (w.event) |e| alloc.free(e);
            if (w.signature) |sig| {
                alloc.free(sig.header);
                alloc.free(sig.prefix);
                if (sig.ts_header) |ts| alloc.free(ts);
                alloc.free(sig.secret_ref);
            }
        },
        .cron => |c| alloc.free(c.schedule),
        .chain => |ch| alloc.free(ch.source),
        .api => {},
    }
}
