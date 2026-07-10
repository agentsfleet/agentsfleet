//! Aggregator root for `zig build test-lib` (the `test-unit-agentsfleet-lib` make
//! target): runs the unit tests of every shared module under `src/lib/` in one
//! pass. Each `src/lib/<name>/` is a named module reused across build graphs;
//! its own tests run here, in the module's own instance, so they reach the
//! internals consumers never see. As `src/lib/` grows, add the new module's
//! barrel below — one line per module. Exception: a module that itself
//! consumes a NAMED module (logging → common; call_deadline → common + log)
//! cannot join this root — one file cannot belong to two modules — so it
//! roots its own compilation under the same `test-lib` step (see build.zig)
//! instead of a file-import line here.

test {
    _ = @import("contract/contract.zig");
    _ = @import("common/clock.zig");
    _ = @import("common/env.zig");
    _ = @import("common/backoff.zig");
}
