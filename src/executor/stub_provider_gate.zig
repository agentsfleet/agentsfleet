//! Comptime alias for the test stub provider module.
//!
//! In `zombied-executor-stub` builds (`build_options.executor_provider_stub`
//! true) this resolves to the real stub. In production and harness builds it
//! resolves to a no-op shim with `unreachable`, so `runner.zig` can call
//! `Module.StubProvider.init(...)` unconditionally and the optimizer strips
//! the dead branch.

const std = @import("std");
const nullclaw = @import("nullclaw");
const build_options = @import("build_options");

const providers = nullclaw.providers;

pub const Module = if (build_options.executor_provider_stub)
    @import("test_stub_provider.zig")
else
    struct {
        pub const StubProvider = struct {
            pub fn init(_: std.mem.Allocator) @This() {
                return .{};
            }
            pub fn provider(self: *@This()) providers.Provider {
                _ = self;
                unreachable;
            }
        };
    };
