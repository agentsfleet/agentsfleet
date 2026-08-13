// Fleet config value types.
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
/// Transient post-create status while the synthetic install steps run. The
/// fleet is born here and flips to `active` once the create path's deferred
/// emission reaches its ready step. App-enforced (the status column already
/// stores arbitrary strings — no DDL); the value is shared verbatim with the
/// dashboard (`AGENTSFLEET_STATUS.INSTALLING` in lib/api/fleets.ts), which reads
/// it to keep the installing indicator visible until it resolves.
pub const S_INSTALLING = "installing";

pub const FleetConfigError = error{
    MissingRequiredField,
    InvalidTriggerType,
    InvalidTriggerSource,
    InvalidCredentialRef,
    InvalidBudget,
    InvalidSignatureConfig,
    RuntimeKeysOutsideBlock,
    UnknownRuntimeKey,
    UsefleetBlockRequired,
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

pub const FleetStatus = enum {
    const Self = @This();

    active,
    paused,
    stopped,
    killed,
    /// Transient post-create state while the synthetic install steps run. Not
    /// runnable (the runner skips it) and not terminal; the create path flips it
    /// to `active` on its ready step.
    installing,

    pub fn toSlice(self: Self) []const u8 {
        return switch (self) {
            .active => S_ACTIVE,
            .paused => S_PAUSED,
            .stopped => S_STOPPED,
            .killed => S_KILLED,
            .installing => S_INSTALLING,
        };
    }

    pub fn fromSlice(s: []const u8) ?FleetStatus {
        if (std.mem.eql(u8, s, S_ACTIVE)) return .active;
        if (std.mem.eql(u8, s, S_PAUSED)) return .paused;
        if (std.mem.eql(u8, s, S_STOPPED)) return .stopped;
        if (std.mem.eql(u8, s, S_KILLED)) return .killed;
        if (std.mem.eql(u8, s, S_INSTALLING)) return .installing;
        return null;
    }

    pub fn isTerminal(self: Self) bool {
        return self == .killed;
    }

    pub fn isRunnable(self: Self) bool {
        return self == .active;
    }
};

pub const FleetTriggerType = enum { webhook, cron, api };
pub const DEFAULT_CRON_TIMEZONE = "UTC";
pub const DEFAULT_CRON_MESSAGE = "Scheduled Fleet run";

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
/// `repositories` is the explicit App-ingress binding (`["owner/repo"]`).
/// Null remains valid for the manual per-fleet webhook route, but App ingress
/// never treats it as an all-repositories subscription.
///
/// `credential_name` is an optional vault-key override. The webhook auth
/// resolver builds the vault row name as `fleet:<credential_name orelse source>`.
/// Lets one workspace store distinct webhook secrets per fleet when two
/// fleets subscribe to the same `source` (e.g. two GitHub orgs).
pub const FleetTrigger = union(FleetTriggerType) {
    webhook: struct {
        source: []const u8,
        events: ?[]const []const u8,
        repositories: ?[]const []const u8 = null,
        credential_name: ?[]const u8 = null,
        signature: ?WebhookSignatureConfig = null,
    },
    cron: struct {
        schedule: []const u8,
        timezone: []const u8,
        message: []const u8,
    },
    api: void,
};

pub const FleetBudget = struct {
    daily_dollars: f64,
    monthly_dollars: ?f64,
};

pub const FleetNetwork = struct {
    allow: []const []const u8,
    read_only: bool = false,
    read_post_paths: []const []const u8 = &.{},
};

/// Frontmatter knobs from `x-agentsfleet.context`. Zero means "auto" — the
/// runner's `ContextBudget.applyDefaults` substitutes `DEFAULT_*` constants.
/// Mirrors the wire-shape of `src/runner/engine/context_budget.zig:ContextBudget`
/// minus the opaque `model` (which lives one level up at `x-agentsfleet.model`).
pub const FleetContextBudget = struct {
    context_cap_tokens: u32 = 0,
    tool_window: u32 = 0,
    memory_checkpoint_every: u32 = 0,
    stage_chunk_threshold: f32 = 0.0,
};

/// Wire spellings of `x-agentsfleet.repository_access` (RULE UFS — the parser,
/// the mint body, and the bundle fixtures all share these two literals).
pub const S_REPOSITORY_ACCESS_READ = "read";
pub const S_REPOSITORY_ACCESS_WRITE = "write";

/// How far a fleet's repository-bearing credentials may reach. `read` mints a
/// token that can fetch history and nothing more; `write` adds what opening a
/// draft Pull Request needs. There is no third value: a fleet that declares no
/// access level mints nothing, rather than inheriting the App installation's
/// full permission set.
pub const RepositoryAccess = enum {
    read,
    write,

    pub fn fromSlice(s: []const u8) ?RepositoryAccess {
        if (std.mem.eql(u8, s, S_REPOSITORY_ACCESS_READ)) return .read;
        if (std.mem.eql(u8, s, S_REPOSITORY_ACCESS_WRITE)) return .write;
        return null;
    }
};

/// The fleet's repository EGRESS binding — which repositories its credentials
/// may reach, and how far.
///
/// Deliberately distinct from `FleetTrigger.webhook.repositories`, which is an
/// INGRESS binding naming which repositories may WAKE the fleet. Overloading one
/// for the other would mean any repository allowed to trigger a fleet was also a
/// repository that fleet could write to.
pub const RepositoryBinding = struct {
    repositories: []const []const u8,
    access: RepositoryAccess,
    /// Trusted Pull Request base for write bindings. Read bindings omit it.
    base_branch: ?[]const u8 = null,
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const FleetConfig = struct {
    const Self = @This();

    name: []const u8,
    triggers: []const FleetTrigger,
    tools: []const []const u8,
    credentials: []const []const u8,
    network: ?FleetNetwork,
    budget: FleetBudget,
    gates: ?config_gates.GatePolicy,
    /// Repository egress binding from top-level `x-agentsfleet.repositories` +
    /// `x-agentsfleet.repository_access`. Null when EITHER is absent — the
    /// GitHub mint then refuses rather than falling back to the installation's
    /// full scope across every repository it covers.
    repository_binding: ?RepositoryBinding,
    // ClaHub skill reference (e.g. "clawhub://queen/lead-hunter@1.0.1").
    // Resolution deferred — stored but not fetched.
    skill: ?[]const u8,
    // Opaque model identifier from `x-agentsfleet.model`. Pass-through: the
    // runner's ContextBudget.model carries it; nothing in this binary
    // interprets it. Empty/null means "fall back to tenant_model_selection" (self-managed).
    model: ?[]const u8,
    // Frontmatter overrides for the context budget knobs. Null means
    // "no `x-agentsfleet.context:` block authored — every knob is auto."
    context: ?FleetContextBudget,

    pub fn deinit(self: *const Self, alloc: Allocator) void {
        alloc.free(self.name);
        for (self.triggers) |t| freeFleetTrigger(alloc, t);
        alloc.free(self.triggers);
        freeStringSlice(alloc, self.tools);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| {
            freeStringSlice(alloc, net.allow);
            freeStringSlice(alloc, net.read_post_paths);
        }
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
        if (self.repository_binding) |b| {
            freeStringSlice(alloc, b.repositories);
            if (b.base_branch) |base| alloc.free(base);
        }
        if (self.skill) |s| alloc.free(s);
        if (self.model) |s| alloc.free(s);
    }
};

// Guards against silent field drift: if a field is added to FleetConfig
// without updating deinit(), @sizeOf changes and this assert fails at compile.
// Trigger storage flipped from inline union to a heap slice — fixed-size
// `[]const FleetTrigger` (16 bytes) replaces the largest-variant
// `FleetTrigger` union. If the layout shifts, update this number rather
// than papering over with a runtime check.
comptime {
    std.debug.assert(@sizeOf(FleetConfig) == 288);
}

/// Authoring metadata extracted from SKILL.md frontmatter (the SOUL file's
/// top-level keys). Required: `name`, `description`, `version`. Optional
/// pass-through fields (`author`, `model`, `when_to_use`) are parsed but not
/// interpreted by the runtime — they exist for skill-host portability. `tags`
/// IS interpreted: it persists to `core.fleets.required_tags` and gates
/// placement (a runner claims the fleet only when `tags ⊆ runner.labels`;
/// see `validRequiredTags` + `fleet.assign.listReadyCandidates`). Cross-file
/// invariant enforced upstream: `SkillMetadata.name == FleetConfig.name`.
pub const SkillMetadata = struct {
    const Self = @This();

    name: []const u8,
    description: []const u8,
    version: []const u8,
    when_to_use: ?[]const u8 = null,
    tags: []const []const u8 = &.{},
    author: ?[]const u8 = null,
    model: ?[]const u8 = null,

    pub fn deinit(self: *const Self, alloc: Allocator) void {
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

/// Placement-tag bounds for core.fleets.required_tags (derived from
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

pub fn freeFleetTrigger(alloc: Allocator, t: FleetTrigger) void {
    switch (t) {
        .webhook => |w| {
            alloc.free(w.source);
            if (w.events) |evs| {
                for (evs) |e| alloc.free(e);
                alloc.free(evs);
            }
            if (w.repositories) |repos| freeStringSlice(alloc, repos);
            if (w.credential_name) |c| alloc.free(c);
            if (w.signature) |sig| {
                alloc.free(sig.header);
                alloc.free(sig.prefix);
                if (sig.ts_header) |ts| alloc.free(ts);
                alloc.free(sig.secret_ref);
            }
        },
        .cron => |c| {
            alloc.free(c.schedule);
            alloc.free(c.timezone);
            alloc.free(c.message);
        },
        .api => {},
    }
}
