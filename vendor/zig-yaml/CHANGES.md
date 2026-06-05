# Vendor patches over upstream zig-yaml

Source: https://github.com/kubkon/zig-yaml (branch `main`)
Pinned upstream commit: `84d747bc80937a08ea1cf76a63fee12c5fb1dd61` (0.3.0-dev)

This directory is a verbatim copy of upstream at the commit above plus the local
patch listed below. Drop the vendor copy and re-pin to upstream once the patch
lands there.

## Patch 1 — Drop the YAML Test Suite conformance step from build.zig

**File:** `build.zig`

**Symptom on Zig 0.16:** Every consumer of zig-yaml fails to build with
`error: root source file struct 'std' has no member named 'StringArrayHashMap'`
(and, earlier, `'fs' has no member named 'cwd'`) — raised from `test/spec.zig`,
the upstream YAML Test Suite conformance harness.

**Root cause:** `build.zig` did `const SpecTest = @import("test/spec.zig");` at
the top level and referenced `SpecTest.create(b)` inside the
`if (enable_spec_tests)` branch. The Zig build script is itself compiled, so that
branch is semantically analyzed even though the option defaults to `false`. The
harness uses `std.StringArrayHashMap` and the old `std.fs.cwd()` shape — both
removed/reshaped in Zig 0.16 — so the broken harness breaks the whole build.

**Fix:** Remove the top-level `@import("test/spec.zig")` and the
`enable_spec_tests` block. We consume only the `yaml` module; the conformance
suite is upstream-test infrastructure we do not run. The library source
(`src/lib.zig` and friends) is already Zig 0.16-clean.
