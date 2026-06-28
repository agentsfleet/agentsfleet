//! run_context.zig — the dependency-injection seam for the engine execute path
//! (M100). `executeInner` used to hard-wire the LLM provider acquisition, which
//! made its success path untestable without a live network. `RunDeps` lifts that
//! one coupling behind a function pointer: production wires the real runtime
//! bundle; a unit test injects a stub provider (or a controlled failure) to drive
//! the engine path offline. The default `RunDeps{}` reproduces today's behaviour
//! exactly, so threading it through is behaviour-preserving.

const std = @import("std");
const nullclaw = @import("nullclaw");
const providers = nullclaw.providers;
const Config = nullclaw.config.Config;
const runner_helpers = @import("runner_helpers.zig");
const credential_request = @import("credential_request.zig");

/// Acquire the LLM provider for one run. The bundle is owned by the caller (it
/// holds the real provider's resources and is `deinit`'d there); the production
/// impl fills it, a stub leaves it empty (its `deinit` then no-ops).
pub const AcquireProviderFn = *const fn (
    alloc: std.mem.Allocator,
    cfg: *Config,
    bundle: *runner_helpers.ProviderBundle,
) anyerror!providers.Provider;

/// Injectable dependencies for the engine run. Every field defaults to the
/// production wiring, so `RunDeps{}` is the live path and tests override only
/// what they need.
pub const RunDeps = struct {
    acquireProvider: AcquireProviderFn = runtimeAcquireProvider,
    /// The child→runner on-demand mint channel (M102 §4), forwarded to the tool
    /// bridge so the policy-aware http tool can mint at the boundary. Null on the
    /// no-session path (tests, register-only) — a mintable placeholder fails closed.
    cred_channel: ?credential_request.Channel = null,
};

/// Production provider acquisition: build the real runtime bundle into `bundle`
/// (caller owns its `deinit`) and hand back the provider interface.
pub fn runtimeAcquireProvider(
    alloc: std.mem.Allocator,
    cfg: *Config,
    bundle: *runner_helpers.ProviderBundle,
) anyerror!providers.Provider {
    return bundle.acquire(alloc, cfg);
}

test {
    _ = @import("run_context_test.zig");
}
