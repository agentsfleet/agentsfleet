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
    InvalidCredentialRef,
    InvalidBudget,
    InvalidSignatureConfig,
    RuntimeKeysOutsideBlock,
    UnknownRuntimeKey,
    UsezombieBlockRequired,
    NameMismatch,
    InvalidNameFormat,
    InvalidVersionFormat,
    InvalidTagFormat,
    /// Field is present but its YAML/JSON type or value is wrong (e.g.
    /// `context: "bad"` where an object is expected, `tool_window: -1`,
    /// `tool_window: true`). Distinct from `MissingRequiredField` so a CI
    /// log clearly distinguishes "you forgot a key" from "you got the
    /// shape wrong."
    InvalidFieldType,
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

pub const ZombieTriggerType = enum { webhook, cron, api };

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
};

pub const ZombieBudget = struct {
    daily_dollars: f64,
    monthly_dollars: ?f64,
};

pub const ZombieNetwork = struct {
    allow: []const []const u8,
};

/// Frontmatter knobs from `x-usezombie.context`. Zero means "auto" — the
/// executor's `applyContextDefaults` substitutes `DEFAULT_*` constants.
/// Mirrors the wire-shape of `executor/context_budget.zig:ContextBudget`
/// minus the opaque `model` (which lives one level up at `x-usezombie.model`).
pub const ZombieContextBudget = struct {
    context_cap_tokens: u32 = 0,
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 0,
    stage_chunk_threshold: f32 = 0.0,
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const ZombieConfig = struct {
    name: []const u8,
    trigger: ZombieTrigger,
    tools: []const []const u8,
    credentials: []const []const u8,
    network: ?ZombieNetwork,
    budget: ZombieBudget,
    gates: ?config_gates.GatePolicy,
    // M2_002: ClaHub skill reference (e.g. "clawhub://queen/lead-hunter@1.0.1")
    // Resolution deferred — stored but not fetched.
    skill: ?[]const u8,
    // Opaque model identifier from `x-usezombie.model`. Pass-through: the
    // executor's ContextBudget.model carries it; nothing in this binary
    // interprets it. Empty/null means "fall back to tenant_providers" (BYOK).
    model: ?[]const u8,
    // Frontmatter overrides for the context budget knobs. Null means
    // "no `x-usezombie.context:` block authored — every knob is auto."
    context: ?ZombieContextBudget,

    pub fn deinit(self: *const ZombieConfig, alloc: Allocator) void {
        alloc.free(self.name);
        freeZombieTrigger(alloc, self.trigger);
        freeStringSlice(alloc, self.tools);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| freeStringSlice(alloc, net.allow);
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
        if (self.skill) |s| alloc.free(s);
        if (self.model) |s| alloc.free(s);
    }
};

// Guards against silent field drift: if a field is added to ZombieConfig
// without updating deinit(), @sizeOf changes and this assert fails at compile.
// `model: ?[]const u8` (16 bytes) + `context: ?ZombieContextBudget`
// (20 bytes payload + 1 byte tag, padded to 24 bytes). Base 272 + 16 + 24 = 312.
comptime {
    std.debug.assert(@sizeOf(ZombieConfig) == 312);
}

/// Authoring metadata extracted from SKILL.md frontmatter (the SOUL file's
/// top-level keys). Required: `name`, `description`, `version`. Optional
/// pass-through fields (`tags`, `author`, `model`, `when_to_use`) are
/// parsed but not interpreted by the runtime — they exist for skill-host
/// portability and ecosystem use. Cross-file invariant enforced upstream:
/// `SkillMetadata.name == ZombieConfig.name`.
pub const SkillMetadata = struct {
    name: []const u8,
    description: []const u8,
    version: []const u8,
    when_to_use: ?[]const u8 = null,
    tags: []const []const u8 = &.{},
    author: ?[]const u8 = null,
    model: ?[]const u8 = null,

    pub fn deinit(self: *const SkillMetadata, alloc: Allocator) void {
        alloc.free(self.name);
        alloc.free(self.description);
        alloc.free(self.version);
        if (self.when_to_use) |s| alloc.free(s);
        freeStringSlice(alloc, self.tags);
        if (self.author) |s| alloc.free(s);
        if (self.model) |s| alloc.free(s);
    }
};

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
        .api => {},
    }
}
