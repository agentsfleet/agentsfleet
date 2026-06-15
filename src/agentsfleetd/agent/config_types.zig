// Agent config value types.
//
// Pure data — no parsing, no I/O. Extracted from config.zig per M28_004.
// Destructors live here so they stay next to the type they free.

const std = @import("std");
const Allocator = std.mem.Allocator;
const config_gates = @import("config_gates.zig");

const S_PAUSED = "paused";
const S_KILLED = "killed";
const S_STOPPED = "stopped";
const S_ACTIVE = "active";

pub const AgentConfigError = error{
    MissingRequiredField,
    InvalidTriggerType,
    InvalidTriggerSource,
    InvalidCredentialRef,
    InvalidBudget,
    InvalidSignatureConfig,
    RuntimeKeysOutsideBlock,
    UnknownRuntimeKey,
    UseagentBlockRequired,
    NameMismatch,
    InvalidNameFormat,
    InvalidVersionFormat,
    InvalidTagFormat,
    /// Field is present but its YAML/JSON type or value is wrong (e.g.
    /// `context: "bad"` where an object is expected, `tool_window: -1`,
    /// `tool_window: true`). Distinct from `MissingRequiredField` so a CI
    /// log clearly distinguishes "you forgot a key" from "you got the
    /// shape wrong." Also covers `triggers[]` shape rejections (length
    /// out-of-bounds, malformed `events`, non-object elements) — the
    /// parser emits a scoped log with the specific cause, the API caller
    /// gets the generic shape-wrong code.
    InvalidFieldType,
};

pub const AgentStatus = enum {
    active,
    paused,
    stopped,
    killed,

    pub fn toSlice(self: AgentStatus) []const u8 {
        return switch (self) {
            .active => S_ACTIVE,
            .paused => S_PAUSED,
            .stopped => S_STOPPED,
            .killed => S_KILLED,
        };
    }

    pub fn fromSlice(s: []const u8) ?AgentStatus {
        if (std.mem.eql(u8, s, S_ACTIVE)) return .active;
        if (std.mem.eql(u8, s, S_PAUSED)) return .paused;
        if (std.mem.eql(u8, s, S_STOPPED)) return .stopped;
        if (std.mem.eql(u8, s, S_KILLED)) return .killed;
        return null;
    }

    pub fn isTerminal(self: AgentStatus) bool {
        return self == .killed;
    }

    pub fn isRunnable(self: AgentStatus) bool {
        return self == .active;
    }
};

pub const AgentTriggerType = enum { webhook, cron, api };

pub const MAX_SIGNATURE_HEADER_LEN: usize = 64;

pub const WebhookSignatureConfig = struct {
    header: []const u8,
    prefix: []const u8,
    ts_header: ?[]const u8 = null,
    secret_ref: []const u8,
};

/// Tagged union for trigger config. Each variant carries only the fields it needs,
/// making invalid states (e.g. webhook without source) unrepresentable.
///
/// `events` is the GitHub-style event-name filter (`["workflow_run"]`).
/// Null means "fire on every event"; non-null asserts an allow-list with
/// length 1..MAX_EVENTS_PER_TRIGGER.
///
/// `credential_name` is an optional vault-key override. The webhook auth
/// resolver builds the vault row name as `agent:<credential_name orelse source>`.
/// Lets one workspace store distinct webhook secrets per agent when two
/// agents subscribe to the same `source` (e.g. two GitHub orgs).
pub const AgentTrigger = union(AgentTriggerType) {
    webhook: struct {
        source: []const u8,
        events: ?[]const []const u8,
        credential_name: ?[]const u8 = null,
        signature: ?WebhookSignatureConfig = null,
    },
    cron: struct { schedule: []const u8 },
    api: void,
};

pub const AgentBudget = struct {
    daily_dollars: f64,
    monthly_dollars: ?f64,
};

pub const AgentNetwork = struct {
    allow: []const []const u8,
};

/// Frontmatter knobs from `x-agentsfleet.context`. Zero means "auto" — the
/// runner's `ContextBudget.applyDefaults` substitutes `DEFAULT_*` constants.
/// Mirrors the wire-shape of `src/runner/engine/context_budget.zig:ContextBudget`
/// minus the opaque `model` (which lives one level up at `x-agentsfleet.model`).
pub const AgentContextBudget = struct {
    context_cap_tokens: u32 = 0,
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 0,
    stage_chunk_threshold: f32 = 0.0,
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const AgentConfig = struct {
    name: []const u8,
    triggers: []const AgentTrigger,
    tools: []const []const u8,
    credentials: []const []const u8,
    network: ?AgentNetwork,
    budget: AgentBudget,
    gates: ?config_gates.GatePolicy,
    // ClaHub skill reference (e.g. "clawhub://queen/lead-hunter@1.0.1").
    // Resolution deferred — stored but not fetched.
    skill: ?[]const u8,
    // Opaque model identifier from `x-agentsfleet.model`. Pass-through: the
    // runner's ContextBudget.model carries it; nothing in this binary
    // interprets it. Empty/null means "fall back to tenant_providers" (self-managed).
    model: ?[]const u8,
    // Frontmatter overrides for the context budget knobs. Null means
    // "no `x-agentsfleet.context:` block authored — every knob is auto."
    context: ?AgentContextBudget,

    pub fn deinit(self: *const AgentConfig, alloc: Allocator) void {
        alloc.free(self.name);
        for (self.triggers) |t| freeAgentTrigger(alloc, t);
        alloc.free(self.triggers);
        freeStringSlice(alloc, self.tools);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| freeStringSlice(alloc, net.allow);
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
        if (self.skill) |s| alloc.free(s);
        if (self.model) |s| alloc.free(s);
    }
};

// Guards against silent field drift: if a field is added to AgentConfig
// without updating deinit(), @sizeOf changes and this assert fails at compile.
// Trigger storage flipped from inline union to a heap slice — fixed-size
// `[]const AgentTrigger` (16 bytes) replaces the largest-variant
// `AgentTrigger` union. If the layout shifts, update this number rather
// than papering over with a runtime check.
comptime {
    std.debug.assert(@sizeOf(AgentConfig) == 216);
}

/// Authoring metadata extracted from SKILL.md frontmatter (the SOUL file's
/// top-level keys). Required: `name`, `description`, `version`. Optional
/// pass-through fields (`author`, `model`, `when_to_use`) are parsed but not
/// interpreted by the runtime — they exist for skill-host portability. `tags`
/// IS interpreted: it persists to `core.agents.required_tags` and gates
/// placement (a runner claims the agent only when `tags ⊆ runner.labels`;
/// see `validRequiredTags` + `fleet.assign.listCandidates`). Cross-file
/// invariant enforced upstream: `SkillMetadata.name == AgentConfig.name`.
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

/// Placement-tag bounds for core.agents.required_tags (derived from
/// SkillMetadata.tags, matched ⊆ runner.labels at lease time): bounded count +
/// per-tag length, so a runaway manifest cannot store an unbounded array.
const MAX_REQUIRED_TAGS: usize = 32;
const MAX_TAG_LEN: usize = 64;

/// True when `tags` is a storable placement set: bounded count, each tag
/// non-empty and within MAX_TAG_LEN. Char-class is intentionally unchecked —
/// runner labels are not validated either and the match is exact-string, so a
/// bad-char tag simply never matches rather than corrupting anything. Callers
/// map false → UZ-REQ-001 (create/patch).
pub fn validRequiredTags(tags: []const []const u8) bool {
    if (tags.len > MAX_REQUIRED_TAGS) return false;
    for (tags) |t| if (t.len == 0 or t.len > MAX_TAG_LEN) return false;
    return true;
}

pub fn freeAgentTrigger(alloc: Allocator, t: AgentTrigger) void {
    switch (t) {
        .webhook => |w| {
            alloc.free(w.source);
            if (w.events) |evs| {
                for (evs) |e| alloc.free(e);
                alloc.free(evs);
            }
            if (w.credential_name) |c| alloc.free(c);
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

test "validRequiredTags: accepts empty/normal sets, rejects over-count and bad lengths" {
    // Empty set is the any-runner identity — must be valid.
    try std.testing.expect(validRequiredTags(&.{}));
    // A normal small capability set.
    try std.testing.expect(validRequiredTags(&.{ "gpu", "us-east" }));

    // Empty-string tag rejected (would store a meaningless label).
    try std.testing.expect(!validRequiredTags(&.{""}));
    try std.testing.expect(!validRequiredTags(&.{ "gpu", "" }));

    // Per-tag length boundary: exactly MAX is accepted, one over is rejected.
    const tag_at_max = "a" ** 64;
    const tag_over_max = "a" ** 65;
    try std.testing.expect(validRequiredTags(&.{tag_at_max}));
    try std.testing.expect(!validRequiredTags(&.{tag_over_max}));

    // Count boundary: exactly MAX accepted, one over rejected.
    var at_max: [32][]const u8 = undefined;
    for (&at_max) |*t| t.* = "x";
    try std.testing.expect(validRequiredTags(&at_max));
    var over_max: [33][]const u8 = undefined;
    for (&over_max) |*t| t.* = "x";
    try std.testing.expect(!validRequiredTags(&over_max));
}
