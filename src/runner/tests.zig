//! Aggregate test root for the zombie-runner binary —
//! `zig build --build-file build_runner.zig test` roots here (not `main.zig`)
//! so the production entry point stays free of test wiring. Importing
//! `main.zig` pulls in the prod module graph and main's own inline tests; the
//! remaining lines force the daemon/ + engine/ modules and their `*_test.zig`
//! files into the test compilation. No pg/redis — same isolation the runner
//! exe ships with. Mirrors `src/zombied/tests.zig`.

test {
    _ = @import("main.zig");
    _ = @import("daemon/control_plane_client.zig");
    _ = @import("daemon/control_plane_client_test.zig");
    _ = @import("daemon/config.zig");
    _ = @import("daemon/loop.zig");
    _ = @import("daemon/loop_test.zig");
    _ = @import("daemon/renew_driver.zig");
    _ = @import("daemon/renew_driver_test.zig");
    _ = @import("common");
    _ = @import("child_supervisor.zig");
    _ = @import("child_supervisor_result.zig");
    _ = @import("child_supervisor_test.zig");
    _ = @import("child_process.zig");
    _ = @import("child_exec.zig");
    _ = @import("cmd/version.zig");
    _ = @import("cmd/args.zig");
    _ = @import("cmd/output.zig");
    _ = @import("cmd/registry.zig");
    _ = @import("cmd/help.zig");
    _ = @import("cmd/status.zig");
    _ = @import("cmd/doctor.zig");
    _ = @import("sandbox_args.zig");
    _ = @import("pipe_proto.zig");
    _ = @import("engine/runner.zig");
    _ = @import("engine/types.zig");
    _ = @import("engine/context_budget.zig");
    _ = @import("engine/tool_bridge.zig");
    _ = @import("engine/cgroup.zig");
    _ = @import("engine/landlock.zig");
    _ = @import("engine/network.zig");
    // W1 runner-daemon coverage
    _ = @import("child_supervisor_edge_test.zig");
    _ = @import("child_supervisor_concurrency_test.zig");
    _ = @import("daemon/renew_driver_edge_test.zig");
    _ = @import("daemon/renew_driver_concurrency_test.zig");
    _ = @import("daemon/control_plane_client_edge_test.zig");
    _ = @import("pipe_proto_edge_test.zig");
    // W2 runner-engine coverage
    _ = @import("child_exec_edge_test.zig");
    _ = @import("sandbox_args_edge_test.zig");
    _ = @import("tool_bridge_resolution_test.zig");
}
