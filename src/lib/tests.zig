//! Aggregator root for `zig build test-lib` (the `test-unit-ziglib` make
//! target): runs the unit tests of every shared module under `src/lib/` in one
//! pass. Each `src/lib/<name>/` is a named module reused across build graphs;
//! its own tests run here, in the module's own instance, so they reach the
//! internals consumers never see. As `src/lib/` grows, add the new module's
//! barrel below — one line per module.

test {
    _ = @import("contract/contract.zig");
}
