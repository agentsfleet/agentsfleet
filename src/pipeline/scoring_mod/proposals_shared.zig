const std = @import("std");
const error_codes = @import("../../errors/codes.zig");

pub const ACTIVE_PROPOSAL_PLACEHOLDER = "[]";
pub const DISALLOWED_PROMPT_FIELD = "system_prompt_appendix";
pub const GENERATION_STATUS_PENDING = "PENDING";
pub const GENERATION_STATUS_READY = "READY";
pub const GENERATION_STATUS_REJECTED = "REJECTED";
pub const PROPOSAL_TARGET_STAGE_BINDING = "stage_binding";
pub const PROPOSAL_TARGET_STAGE_INSERT = "stage_insert";
pub const PROPOSAL_ACTOR = "proposal_generator";
pub const COMPILE_ENGINE_DETERMINISTIC_V1 = "deterministic-v1";
pub const STATUS_REJECTED = "REJECTED";
pub const STATUS_PENDING_REVIEW = "PENDING_REVIEW";
pub const STATUS_APPROVED = "APPROVED";
pub const STATUS_VETO_WINDOW = "VETO_WINDOW";
pub const STATUS_APPLIED = "APPLIED";
pub const STATUS_CONFIG_CHANGED = "CONFIG_CHANGED";
pub const STATUS_VETOED = "VETOED";
pub const TRUST_LEVEL_UNEARNED = "UNEARNED";
pub const TRUST_LEVEL_TRUSTED = "TRUSTED";
pub const DEFAULT_RECONCILE_BATCH_LIMIT: u32 = 32;
pub const AUTO_APPLY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
pub const MANUAL_PROPOSAL_EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const APPLIED_BY_SYSTEM_AUTO = "system:auto";
pub const APPLIED_BY_OPERATOR_PREFIX = "operator:";
pub const REJECTION_REASON_COMPILE_FAILED = "COMPILE_FAILED";
pub const REJECTION_REASON_ACTIVATE_FAILED = "ACTIVATE_FAILED";
pub const REJECTION_REASON_CONFIG_CHANGED_SINCE_PROPOSAL = "CONFIG_CHANGED_SINCE_PROPOSAL";
pub const REJECTION_REASON_EXPIRED = "EXPIRED";
pub const VALIDATION_STATUS_AUTO_APPLIED_JSON = "{\"status\":\"auto_applied\"}";
pub const JSON_KEY_TARGET_FIELD = "target_field";
pub const JSON_KEY_CURRENT_VALUE = "current_value";
pub const JSON_KEY_PROPOSED_VALUE = "proposed_value";
pub const JSON_KEY_RATIONALE = "rationale";
pub const JSON_KEY_AGENT_ID = "agent_id";
pub const JSON_KEY_STAGE_ID = "stage_id";
pub const JSON_KEY_ROLE = "role";
pub const JSON_KEY_SKILL = "skill";
pub const JSON_KEY_SKILL_ID = "skill_id";
pub const JSON_KEY_INSERT_BEFORE_STAGE_ID = "insert_before_stage_id";
pub const JSON_KEY_ARTIFACT_NAME = "artifact_name";
pub const JSON_KEY_COMMIT_MESSAGE = "commit_message";
pub const JSON_KEY_GATE = "gate";
pub const JSON_KEY_ON_PASS = "on_pass";
pub const JSON_KEY_ON_FAIL = "on_fail";

pub const ProposalError = error{
    InvalidProposalJson,
    ProposalNotArray,
    ProposalChangeNotObject,
    MissingTargetField,
    UnsupportedTargetField,
    MissingStageId,
    MissingRole,
    MissingInsertBeforeStageId,
    DisallowedProposalField,
    UnregisteredAgentRef,
    InvalidSkillRef,
    EntitlementSkillNotAllowed,
    UnknownStageRef,
    DuplicateStageRef,
    ProposalWouldNotCompile,
    EntitlementProfileLimit,
    EntitlementStageLimit,
    NoValidProposalTemplate,
};

pub const ProposalTriggerReason = enum {
    declining_score,
    sustained_low_score,

    pub fn label(self: ProposalTriggerReason) []const u8 {
        return switch (self) {
            .declining_score => "DECLINING_SCORE",
            .sustained_low_score => "SUSTAINED_LOW_SCORE",
        };
    }
};

pub const ApprovalMode = enum {
    auto,
    manual,

    pub fn label(self: ApprovalMode) []const u8 {
        return switch (self) {
            .auto => "AUTO",
            .manual => "MANUAL",
        };
    }
};

pub const RollingTrigger = struct {
    reason: ProposalTriggerReason,
};

pub const ActiveConfigContext = struct {
    trust_level: []u8,
    config_version_id: []u8,

    pub fn deinit(self: *ActiveConfigContext, alloc: std.mem.Allocator) void {
        alloc.free(self.trust_level);
        alloc.free(self.config_version_id);
    }
};

pub const PendingProposal = struct {
    proposal_id: []u8,
    agent_id: []u8,
    workspace_id: []u8,
    config_version_id: []u8,
    trigger_reason: []u8,

    pub fn deinit(self: *PendingProposal, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.agent_id);
        alloc.free(self.workspace_id);
        alloc.free(self.config_version_id);
        alloc.free(self.trigger_reason);
    }
};

pub const GenerationReconcileResult = struct {
    ready: u32 = 0,
    rejected: u32 = 0,
};

pub const AutoApprovalReconcileResult = struct {
    applied: u32 = 0,
    config_changed: u32 = 0,
    rejected: u32 = 0,
    expired: u32 = 0,
};

pub const ProposalSummary = struct {
    proposal_id: []u8,
    trigger_reason: []u8,
    proposed_changes: []u8,
    config_version_id: []u8,
    approval_mode: []u8,
    status: []u8,
    auto_apply_at: ?i64,
    created_at: i64,
    updated_at: i64,

    pub fn deinit(self: *ProposalSummary, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.trigger_reason);
        alloc.free(self.proposed_changes);
        alloc.free(self.config_version_id);
        alloc.free(self.approval_mode);
        alloc.free(self.status);
    }
};

pub const ManualProposalSummary = ProposalSummary;

pub const ProposalLookup = struct {
    proposal_id: []u8,
    agent_id: []u8,
    workspace_id: []u8,
    config_version_id: []u8,
    proposed_changes: []u8,

    pub fn deinit(self: *ProposalLookup, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.agent_id);
        alloc.free(self.workspace_id);
        alloc.free(self.config_version_id);
        alloc.free(self.proposed_changes);
    }
};

pub const AppliedProposalTelemetry = struct {
    proposal_id: []u8,
    agent_id: []u8,
    workspace_id: []u8,
    trigger_reason: []u8,
    approval_mode: []u8,
    fields_changed: [][]u8,

    pub fn deinit(self: *AppliedProposalTelemetry, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.agent_id);
        alloc.free(self.workspace_id);
        alloc.free(self.trigger_reason);
        alloc.free(self.approval_mode);
        for (self.fields_changed) |field| alloc.free(field);
        alloc.free(self.fields_changed);
    }
};

pub const ImprovementStalledAlert = struct {
    proposal_id: []u8,

    pub fn deinit(self: *ImprovementStalledAlert, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
    }
};

pub const ImprovementReport = struct {
    agent_id: []u8,
    trust_level: []u8,
    improvement_stalled_warning: bool,
    proposals_generated: u32,
    proposals_approved: u32,
    proposals_vetoed: u32,
    proposals_rejected: u32,
    proposals_applied: u32,
    avg_score_delta_per_applied_change: ?f64,
    current_tier: ?[]const u8,
    baseline_tier: ?[]const u8,

    pub fn deinit(self: *ImprovementReport, alloc: std.mem.Allocator) void {
        alloc.free(self.agent_id);
        alloc.free(self.trust_level);
    }
};

pub fn rejectionCodeForError(err: anyerror) []const u8 {
    return switch (err) {
        ProposalError.InvalidProposalJson => error_codes.ERR_PROPOSAL_INVALID_JSON,
        ProposalError.ProposalNotArray => error_codes.ERR_PROPOSAL_NOT_ARRAY,
        ProposalError.ProposalChangeNotObject => error_codes.ERR_PROPOSAL_CHANGE_NOT_OBJECT,
        ProposalError.MissingTargetField => error_codes.ERR_PROPOSAL_MISSING_TARGET_FIELD,
        ProposalError.UnsupportedTargetField => error_codes.ERR_PROPOSAL_UNSUPPORTED_TARGET_FIELD,
        ProposalError.MissingStageId => error_codes.ERR_PROPOSAL_MISSING_STAGE_ID,
        ProposalError.MissingRole => error_codes.ERR_PROPOSAL_MISSING_ROLE,
        ProposalError.MissingInsertBeforeStageId => error_codes.ERR_PROPOSAL_MISSING_INSERT_BEFORE_STAGE_ID,
        ProposalError.DisallowedProposalField => error_codes.ERR_PROPOSAL_DISALLOWED_FIELD,
        ProposalError.UnregisteredAgentRef => error_codes.ERR_PROPOSAL_UNREGISTERED_AGENT_REF,
        ProposalError.InvalidSkillRef => error_codes.ERR_PROPOSAL_INVALID_SKILL_REF,
        ProposalError.EntitlementSkillNotAllowed => error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED,
        ProposalError.UnknownStageRef => error_codes.ERR_PROPOSAL_UNKNOWN_STAGE_REF,
        ProposalError.DuplicateStageRef => error_codes.ERR_PROPOSAL_DUPLICATE_STAGE_REF,
        ProposalError.ProposalWouldNotCompile => error_codes.ERR_PROPOSAL_WOULD_NOT_COMPILE,
        ProposalError.EntitlementProfileLimit => error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT,
        ProposalError.EntitlementStageLimit => error_codes.ERR_ENTITLEMENT_STAGE_LIMIT,
        ProposalError.NoValidProposalTemplate => error_codes.ERR_PROPOSAL_NO_VALID_TEMPLATE,
        else => error_codes.ERR_PROPOSAL_GENERATION_FAILED,
    };
}
