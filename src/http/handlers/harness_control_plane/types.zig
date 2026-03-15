pub const PutSourceInput = struct {
    agent_id: ?[]const u8 = null,
    name: ?[]const u8 = null,
    source_markdown: []const u8,
};

pub const PutSourceOutput = struct {
    agent_id: []u8,
    config_version_id: []const u8,
    version: i32,
};

pub const CompileInput = struct {
    agent_id: ?[]const u8 = null,
    config_version_id: ?[]const u8 = null,
};

pub const CompileOutput = struct {
    compile_job_id: []const u8,
    agent_id: []const u8,
    config_version_id: []const u8,
    is_valid: bool,
    validation_report_json: []const u8,
};

pub const ActivateInput = struct {
    config_version_id: []const u8,
    activated_by: ?[]const u8 = null,
};

pub const ActivateOutput = struct {
    agent_id: []const u8,
    config_version_id: []const u8,
    run_snapshot_config_version: []const u8,
    activated_by: []const u8,
    activated_at: i64,
};

pub const ActiveOutput = struct {
    source: []const u8,
    agent_id: ?[]const u8,
    config_version_id: ?[]const u8,
    run_snapshot_config_version: ?[]const u8,
    active_at: ?i64,
    profile_json: []u8,
};

pub const ControlPlaneError = error{
    InvalidRequest,
    InvalidIdShape,
    WorkspaceNotFound,
    ProfileNotFound,
    ProfileInvalid,
    CompileFailed,
    EntitlementMissing,
    EntitlementProfileLimit,
    EntitlementStageLimit,
    EntitlementSkillNotAllowed,
    CreditExhausted,
};
