pub const PutSourceInput = struct {
    profile_id: ?[]const u8 = null,
    name: ?[]const u8 = null,
    source_markdown: []const u8,
};

pub const PutSourceOutput = struct {
    profile_id: []u8,
    profile_version_id: []const u8,
    version: i32,
};

pub const CompileInput = struct {
    profile_id: ?[]const u8 = null,
    profile_version_id: ?[]const u8 = null,
};

pub const CompileOutput = struct {
    compile_job_id: []const u8,
    profile_id: []const u8,
    profile_version_id: []const u8,
    is_valid: bool,
    validation_report_json: []const u8,
};

pub const ActivateInput = struct {
    profile_version_id: []const u8,
    activated_by: ?[]const u8 = null,
};

pub const ActivateOutput = struct {
    profile_id: []const u8,
    profile_version_id: []const u8,
    run_snapshot_version: []const u8,
    activated_by: []const u8,
    activated_at: i64,
};

pub const ActiveOutput = struct {
    source: []const u8,
    profile_id: ?[]const u8,
    profile_version_id: ?[]const u8,
    run_snapshot_version: ?[]const u8,
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
};
