const types = @import("harness_control_plane/types.zig");
const put_source = @import("harness_control_plane/put_source.zig");
const compile_mod = @import("harness_control_plane/compile.zig");
const activate_mod = @import("harness_control_plane/activate.zig");
const get_active_mod = @import("harness_control_plane/get_active.zig");

pub const PutSourceInput = types.PutSourceInput;
pub const PutSourceOutput = types.PutSourceOutput;
pub const CompileInput = types.CompileInput;
pub const CompileOutput = types.CompileOutput;
pub const ActivateInput = types.ActivateInput;
pub const ActivateOutput = types.ActivateOutput;
pub const ActiveOutput = types.ActiveOutput;
pub const ControlPlaneError = types.ControlPlaneError;

pub const putSource = put_source.putSource;
pub const compileProfile = compile_mod.compileProfile;
pub const activateProfile = activate_mod.activateProfile;
pub const getActiveProfile = get_active_mod.getActiveProfile;

comptime {
    _ = @import("harness_control_plane/tests.zig");
}
