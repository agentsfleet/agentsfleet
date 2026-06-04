# M82_001: Migrate the toolchain from Zig 0.15.2 to 0.16.0

**Prototype:** v2.0.0
**Milestone:** M82
**Workstream:** 001
**Date:** Jun 02, 2026
**Status:** DONE
**Priority:** P2 — toolchain bump, no customer-facing behaviour change.
**Categories:** API, INFRA
**Batch:** B1 — single atomic landing. The `std.http.Client` migration originally carved out to M82_002 is **absorbed into B1** (see Discovery, Jun 03, 2026): 0.16 makes `io: Io` a required, no-default field on `std.http.Client`, so the build cannot go green on 0.16 while those sites stay on the 0.15 constructor — the split was infeasible. M82_002 is dissolved.
**Branch:** feat/m82-zig-016-toolchain
**Depends on:** None hard. Sequenced PENDING behind M80 (runner fleet) completion — see Overview for the forcing-function trigger.
**Provenance:** agent-generated (pre-spec, `spike/zig-0.16-feasibility` — feasibility proven: full dependency graph compiles on 0.16). **Correction (Jun 03, 2026):** the spike's "only mechanical `std` renames remain in our own source" claim was proven for the *dependency graph*, not the full `src/` tree — the spike diff touched zero `std.http.Client` sites and never hit the 0.16 `io: Io` requirement. That migration is real work and is now in-scope (§3b).

> **Provenance is load-bearing.** This spec was authored from a live spike, not from reading alone. The dependency-graph claims (§1) are compile-verified on the spike branch; the source-migration counts (§2–§3) are `git grep` calibrations, not estimates. Re-verify the counts at PLAN — they drift as `main` advances.

**Canonical architecture:** Greenfield for the wall-clock abstraction (`src/lib/common/clock.zig` — no existing clock helper; all sites call `std.time.*` directly today). No `docs/architecture/` doc governs toolchain pinning; dependency-pin conventions live as comments in `build.zig.zon` and `vendor/*/CHANGES.md`.

---

## Implementing agent — read these first

1. `spike/zig-0.16-feasibility` branch (this repo) — the proven foundation. `build.zig.zon` re-pins, `vendor/httpz/` re-vendor + UAF patch, `vendor/zig-yaml/` fork are already built there. Batch 1 starts by reconciling this branch onto a fresh feature branch, NOT from scratch.
2. `vendor/httpz/CHANGES.md` — the UAF patch contract (stop-before-deinit in non-blocking `Worker.deinit`). The patch is verified still-required on upstream's `zig-0.16` branch; it must port forward verbatim.
3. `docs/ZIG_RULES.md` + `docs/greptile-learnings/RULES.md` (RULE ZAL, ORP, XCC, UFS) — the Zig discipline this diff trips; RULE ZAL is itself 0.15-pinned and must be reconciled (§5).
4. `playbooks/013_ci_zig_images/` — the `ci-zig-*` image bake process; `versions.env` is the single source of truth for the pinned Zig version + SHAs.
5. https://ziglang.org/download/0.16.0/release-notes.html — the canonical list of removed/reshaped `std` APIs (`std.time.milliTimestamp`/`nanoTimestamp` removal → Io `Clock`; `GeneralPurposeAllocator` → `DebugAllocator`; `fixedBufferStream` → `Io.Writer.fixed`).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `build(m82): migrate toolchain to Zig 0.16.0 (deps, std renames, CI)`
- **Intent (one sentence):** Move the entire build off Zig 0.15.2 onto 0.16.0 — every dependency, our own source, and CI — with zero behaviour change, so the codebase tracks the current stable toolchain.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. A mismatch with the Intent above → STOP and reconcile before any edit. Re-run the §2/§3 calibration greps — the call-site counts below are point-in-time.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specifically trips:
  - **RULE ORP** (cross-layer orphan sweep) — the `milliTimestamp`/`nanoTimestamp`→`clock` redirect is a repo-wide rename; orphan sweep must show **0** residual `std.time.milliTimestamp`/`nanoTimestamp` in `src/`.
  - **RULE ZAL** (Zig 0.15 ArrayList API) — **this rule is 0.15-pinned and this migration obsoletes it.** Reconcile, do not silently violate (§5). Spec-vs-rule conflict resolved by amending the rule, per the migration's nature.
  - **RULE XCC** (cross-compile before commit) — both Linux targets must build on 0.16.
  - **RULE UFS** (string/numeric literals are constants) — the pinned Zig version + SHAs are single-sourced in `versions.env`; the clock unit conversions use named `std.time.ns_per_ms` constants, not magic numbers.
  - **RULE TST-NAM / milestone-free code** — no `M82`/`§` IDs in any `.zig`/`.sh`/test name.
  - **RULE NDC / NLR / NLG** — no dead code at write time; vendor `CHANGES.md` uses "drop when upstream lands" framing (not "legacy"), pre-2.0 compliant.
  - **RULE IMS** — `[]const u8` for immutable data through the clock/format edits.
- **`docs/ZIG_RULES.md`** — all `*.zig` edits: pg-drain lifecycle (untouched here), tagged-union results, multi-step `errdefer`, cross-compile.
- **`docs/LOGGING_STANDARD.md`** — the logging hot path (`src/lib/logging/mod.zig`) changes how it sources the timestamp; emit shape must not change.
- **`docs/LIFECYCLE_PATTERNS.md`** — `vendor/httpz` `Worker.deinit` (the UAF patch) and the new `clock.zig` init/deinit-free shape.
- **`docs/AUTH.md`** — **mandatory read for §3b**: `clerk_backend.zig` and `jwks.zig` are auth-boundary files. The migration threads `io` only; credential/token logic is untouched. Any deviation STOPs and surfaces to Indy.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — pervasive `*.zig` edits | Read `docs/ZIG_RULES.md`; cross-compile both linux targets (RULE XCC) before commit. |
| PUB / Struct-Shape | **yes** — new `src/lib/common/clock.zig` pub surface | Own shape verdict: free-function module (`nowMillis`/`nowNanos`), no struct/inheritance; `zlint unused-decls` covers consumer-grep. |
| File & Function Length (≤350/≤50/≤70) | **yes** — new file + net-adds across the sweep | `clock.zig` is tiny; the timestamp sweep is in-place substitution (no net growth). Watch `logging/mod.zig` and `metrics_workspace.zig` near caps. |
| UFS (repeated/semantic literals) | **yes** — pinned version/SHA, clock unit math | Version + SHAs single-sourced in `versions.env`; unit conversions via `std.time.ns_per_ms`/`ms_per_s` named constants. |
| LOGGING | **yes** — `logging/mod.zig` timestamp source change | Emit envelope shape and fields unchanged; only the timestamp *source* swaps to `clock`. |
| LIFECYCLE | **yes** — httpz `Worker.deinit` patch + new module | UAF patch re-applied verbatim; `clock.zig` holds no heap/handle (no init/deinit). |
| MILESTONE-ID | **yes** — source/config edits outside `docs/` | No `M82`/`§`/dim IDs in code, tests, workflows (RULE TST-NAM). |
| SCHEMA / ERROR REGISTRY / UI / DESIGN TOKEN | **no** | No schema, no new error codes (http.Client errors deferred to M82_002), no UI. |

---

## Overview

**Goal (testable):** `zig build`, `make test`, `make test-integration`, `make memleak`, and `zig build -Dtarget={x86_64,aarch64}-linux` all pass on Zig 0.16.0 with zero `std.time.milliTimestamp`/`nanoTimestamp` references remaining in `src/`, and CI runs the `:0.16.0` images.

**Problem:** The toolchain is pinned to Zig 0.15.2 while 0.16.0 is the current stable (released Apr 13, 2026). Our own libraries (`posthog-zig`, `nullclaw`) have already moved to 0.16; staying on 0.15.2 splits the toolchain across the org and blocks adopting any 0.16-only dependency. There is no behaviour to change — this is pure toolchain currency.

**Solution summary:** Re-pin every dependency to its 0.16-compatible revision (two needing a vendored fork — `httpz`, `zig-yaml`), introduce a single `clock` helper to absorb the removal of `std.time.milliTimestamp`/`nanoTimestamp`, mechanically redirect ~206 wall-clock call sites to it, apply the handful of remaining `std` renames (`DebugAllocator`, `Io.Writer.fixed`, unmanaged HashMaps), thread a `std.Io.Threaded` instance to the `std.http.Client` sites (0.16 requires `io: Io`), and cut CI over to freshly-baked `:0.16.0` images. **Scope note (corrected Jun 03, 2026 — see Discovery):** 0.16 is a full `std.Io` reform. `io` is **forced into ~80 sites** (66 mutex `lock/unlock/wait` + ~17 runner `std.fs.*Absolute` + 5 `http.Client` + 2 `tcpConnectToHost`) regardless of approach, because `std.Io.Mutex.lock`/`unlock` now take `io`. Decision = **Option B (hybrid)**: zombied carries `io` as a field on the `http_handler.Context` DI seam (explicit, testable — not a bare global); the runner threads `io` signature-only through `runLoop` (single-lease stays sequential, no async refactor — sets the seam the future async scheduler owns). Wall-clock stays a **direct syscall, not `Io`** (`clock.zig`), keeping `Io` out of the 206 timestamp sites. Full async adoption (concurrent execution, `io.async`) remains out of scope — a future scheduler milestone, not this bump.

**Forcing-function trigger (historical — now active):** Authored PENDING as a no-deadline migration with real churn and one runtime risk (upstream's `http.zig` 0.16 branch self-describes as experimental). **Pulled active Jun 03, 2026 by Indy's "migrate all to 0.16" directive** (Discovery) — the toolchain currency is wanted now, not parked behind a later forcing function.

---

## Prior-Art / Reference Implementations

- **Dependency forks** → `vendor/httpz/` is the established in-repo pattern (verbatim upstream copy + documented `CHANGES.md` patch). `vendor/zig-yaml/` mirrors it exactly. Alignment: identical structure; divergence: none.
- **Our own 0.16 migrations** → `usezombie/posthog-zig` v0.2.0 and `nullclaw` v2026.5.29 are the reference for the `std.Io`/Clock idioms (posthog's `init` gained an `io` param; mirror that call-site shape at `src/zombied/cmd/preflight.zig`).
- **Clock helper** → no prior art in-repo; greenfield. Shape it as a minimal free-function module wrapping the 0.16 wall-clock primitive (Io `Clock` via a retained process `Io`, or direct syscall) — decided at PLAN against the 0.16 release notes.

---

## Files Changed (blast radius)

> The timestamp redirect spans ~95 files; enumerating each is impractical and drift-prone, so it is scoped by a **deterministic command** rather than a static list. The agent may edit any `src/**/*.zig` returned by that grep without further override — that IS the scoped set.

| File | Action | Why |
|------|--------|-----|
| `build.zig.zon` | EDIT | Re-pin all deps to 0.16 revs; `minimum_zig_version` → 0.16.0 |
| `vendor/httpz/**` | EDIT | Re-vendor from upstream `zig-0.16` branch (40be022); re-apply UAF stop-before-deinit patch |
| `vendor/zig-yaml/**` | CREATE | New vendored fork; library source 0.16-clean, conformance harness dropped from its `build.zig` |
| `src/lib/common/clock.zig` | CREATE | The wall-clock helper (`nowMillis`/`nowNanos`) absorbing the `std.time.*` removal |
| `src/lib/common/constants.zig` | EDIT | Export the clock module via the `common` named module (Zig boundary law) |
| `all src/**/*.zig` matching `git grep -l 'std\.time\.\(milli\|nano\)Timestamp'` | EDIT | Mechanical redirect to `clock.nowMillis`/`nowNanos` (~95 files — the orphan grep IS the scope) |
| `src/lib/logging/mod.zig`, `envelope.zig` | EDIT | Timestamp source → `clock`; `fixedBufferStream` → `Io.Writer.fixed` |
| `src/zombied/observability/metrics_workspace.zig`, `metrics_runner_test.zig`, `cmd/doctor.zig` | EDIT | `fixedBufferStream` → `Io.Writer.fixed` |
| `src/zombied/main.zig` | EDIT | `GeneralPurposeAllocator` → `DebugAllocator`; clock for log timestamps |
| `src/zombied/cmd/preflight.zig` | EDIT | `posthog.init` gains the `io` param (v0.2.0 signature) |
| `src/runner/daemon/control_plane_client.zig`, `src/zombied/observability/otel_logs.zig` | EDIT | `std.http.Client` gains the required `io: Io` field; thread a `std.Io.Threaded` to each construction (§3b) |
| `src/zombied/auth/clerk_backend.zig`, `src/zombied/auth/jwks.zig` | EDIT | **Auth boundary** — same `io: Io` threading; read `docs/AUTH.md`, `/review` these two specifically. Mechanical Io-plumbing only; any auth-logic change STOPs and surfaces (§3b) |
| http.Client test harnesses (`test_harness_server.zig`, `test_http_message.zig`, `cross_workspace_idor_test.zig`) | EDIT | Same `io: Io` threading in the test construction sites |
| `.github/workflows/{test,lint,memleak,bench,cross-compile,deploy-dev,release,test-integration}.yml` | EDIT | `:0.15.2` image tags + `setup-zig` version → `0.16.0` (9 files) |
| `playbooks/013_ci_zig_images/versions.env` | EDIT | `ZIG_VERSION` 0.15.2 → 0.16.0 + four new SHA256s; rebuild/push the three base images |
| `docs/greptile-learnings/RULES.md` | EDIT | Reconcile RULE ZAL (0.15 ArrayList API → 0.16) — §5 |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One atomic workstream (B1). Deps + clock helper + mechanical `std` renames + `std.http.Client` `io`-threading + CI, landing together. The original two-workstream split (M82_002 for `std.http.Client`) was **dissolved** once 0.16's required `io: Io` field proved the build cannot go green without those sites — a deferral that breaks the build is not a deferral. Within B1, the clock migration (§2) and the http.Client `io`-threading (§3b) are their own Sections: the former is the largest mechanical change (orphan-sweep discipline), the latter the only one crossing the auth boundary (extra `docs/AUTH.md` + `/review` scrutiny).
- **Security-split rule, reconciled:** the house rule (security-boundary work gets its own spec+PR) is satisfied differently here — `clerk_backend`/`jwks` changes are *mechanical `io`-plumbing forced by the compiler*, not auth-logic edits, and they cannot be separated from a green build. The rule's intent (no auth-logic riding an unrelated diff) is honoured by the §3b STOP-and-surface guard: any change beyond threading `io` halts and goes to Indy.
- **Alternatives considered:** (a) **Keep `http.Client` in a follow-up (M82_002)** — rejected: infeasible, the 0.16 `io: Io` requirement breaks the build without it. (b) **Thread `std.Io` through every timestamp call site** instead of a clock helper — rejected: that propagates `io` through ~95 files and up their call trees; the clock helper uses a direct syscall and localizes the change. (c) **Adopt `std.Io` as the daemon's central runtime** — rejected for this PR: prototype-scale refactor, not toolchain currency (Out of Scope). (d) **Don't vendor zig-yaml; pin upstream main** — rejected: upstream's broken conformance harness breaks our build (proven on the spike); vendoring is the only way to consume the clean library today.
- **Patch-vs-refactor verdict:** this is a **patch** (toolchain currency, behaviour-preserving) with one small **new abstraction** (the clock helper) that is itself the minimal change. The larger refactor — adopting `std.Io` as a first-class threaded dependency across the daemon — is real future work but explicitly out of scope; named in Out of Scope.

---

## Sections (implementation slices)

### §1 — Dependency graph on 0.16

Re-pin every dependency to a 0.16-compatible revision and stand the two fork-requiring deps back up. This is the foundation; nothing else compiles until it lands. **Implementation default:** reconcile the proven `spike/zig-0.16-feasibility` branch onto the feature branch rather than re-deriving — the SHAs and patches are already verified there.

- **Dimension 1.1** — `build.zig.zon` re-pinned (nullclaw v2026.5.29, pg.zig 0.16 master commit, posthog v0.2.0, zbench `zig-0.16.0` branch) and `minimum_zig_version = "0.16.0"` → Test `test_dep_graph_resolves_on_016`
- **Dimension 1.2** — `vendor/httpz/` re-vendored from upstream `zig-0.16` (40be022); UAF stop-before-deinit patch present in non-blocking `Worker.deinit` → Test `test_httpz_worker_deinit_stops_pool`
- **Dimension 1.3** — `vendor/zig-yaml/` fork: library `yaml` module compiles; conformance-test wiring removed from its `build.zig` → Test `test_yaml_frontmatter_parses_on_016`
- **Dimension 1.4** — `posthog.init` call at `preflight.zig` passes the v0.2.0 `io` parameter → Test `test_telemetry_init_compiles_with_io`

### §2 — Wall-clock migration (keystone)

`std.time.milliTimestamp`/`nanoTimestamp` are removed in 0.16. Introduce one clock helper and redirect every site. This is the largest mechanical change and the one most exposed to RULE ORP (a missed site = silent build break or, worse, a residual that no longer compiles). **Implementation default:** a free-function `clock` module wrapping the 0.16 wall-clock primitive — the agent picks Io-`Clock`-via-retained-`Io` vs direct syscall at PLAN from the release notes, whichever localizes blast radius.

- **Dimension 2.1** — `src/lib/common/clock.zig` exposes `nowMillis() i64` and `nowNanos() i128` over the 0.16 primitive, unit math via `std.time.ns_per_ms` constants → Test `test_clock_now_millis_is_wall_time`
- **Dimension 2.2** — all 175 `std.time.milliTimestamp()` sites redirected to `clock.nowMillis()`; orphan sweep clean → Test `test_no_residual_milli_timestamp`
- **Dimension 2.3** — all 31 `std.time.nanoTimestamp()` sites redirected to `clock.nowNanos()`; orphan sweep clean → Test `test_no_residual_nano_timestamp`
- **Dimension 2.4** — logging hot path (`logging/mod.zig`) sources its envelope timestamp from `clock`, emit shape unchanged → Test `test_log_envelope_timestamp_via_clock`

### §3 — Remaining mechanical std renames

The small, well-bounded 0.16 renames. Each is a verbatim substitution surfaced by a grep.

- **Dimension 3.1** — `std.heap.GeneralPurposeAllocator` → `std.heap.DebugAllocator` (3 sites) → Test `test_builds_with_debug_allocator`
- **Dimension 3.2** — `std.io.fixedBufferStream` → `std.Io.Writer.fixed` (10 sites: logging/metrics/doctor) → Test `test_fixed_writer_emits_expected_bytes`
- **Dimension 3.3** — `StringHashMap`/`AutoHashMap` unmanaged-API reconciliation (~6 sites) → Test `test_hashmap_unmanaged_round_trip`
- **Dimension 3.4** — ArrayList 0.16 API reconciled across the diff (RULE ZAL surface) → Test `test_arraylist_016_api_compiles`

### §3b — `std.http.Client` → 0.16 (`io: Io` threading)

0.16 makes `io: Io` a **required, no-default field** on `std.http.Client` (alongside `allocator`). Every site constructs it as `.{ .allocator = alloc }`, which no longer compiles — so this is mandatory for a green build, not optional polish. Stand up one `std.Io.Threaded` (blocking-threaded backend; the daemon makes synchronous outbound HTTP calls) and thread it to each `std.http.Client` construction. This is the work the dissolved M82_002 named; it is mechanical `Io`-plumbing, **not** an auth-logic change. The two auth-boundary sites get extra scrutiny.

- **Dimension 3b.1** — `std.Io.Threaded` stood up and `std.http.Client` constructions take `.io`; non-auth prod sites (`control_plane_client.zig`, `otel_logs.zig`) build + behave identically → Test `test_http_client_fetch_with_io`
- **Dimension 3b.2** — auth-boundary sites (`clerk_backend.zig`, `jwks.zig`) threaded the same `io`; `docs/AUTH.md` read; emit/verify behaviour unchanged; null-key/no-token paths still gate cleanly → Test `test_auth_http_client_io_threaded`
- **Dimension 3b.3** — http.Client test harnesses (`test_harness_server.zig`, `test_http_message.zig`, `cross_workspace_idor_test.zig`) threaded `io`; integration suite green → Test `test_integration_http_round_trip_on_016`

### §4 — CI / infra cutover

Cut the build pipeline over to 0.16. The `ci-zig-*` images are self-baked, so the image bump precedes the workflow flip.

- **Dimension 4.1** — `versions.env` → `ZIG_VERSION=0.16.0` + four refreshed SHA256s; the three base images (alpine/ubuntu/debian-trixie) rebuilt and pushed → Test `test_ci_images_pin_016`
- **Dimension 4.2** — all 9 workflow files flipped (`:0.15.2`→`:0.16.0` image tags + `mlugg/setup-zig` `version:` lines); no `0.15.2` string remains → Test `test_no_residual_0152_in_ci`
- **Dimension 4.3** — cross-compile both targets green on 0.16 (RULE XCC) → Test `test_cross_compile_both_linux_targets`

### §5 — Rule reconciliation

RULE ZAL is explicitly "Zig 0.15 ArrayList API." Leaving it post-migration is a standing contradiction. Amend the rule (the migration is the legitimate reason to change the constant).

- **Dimension 5.1** — RULE ZAL updated in `docs/greptile-learnings/RULES.md` to the 0.16 ArrayList API (or retired with a pointer), no "0.15" references stranded → Test `test_rule_zal_references_016`
- **Dimension 5.2** — zig-yaml fix upstreamed: PR opened against `kubkon/zig-yaml` to fix `test/spec.zig` for 0.16 (parallel, external — tracked in Discovery, NOT gating this PR) → tracked, no in-repo test

---

## Interfaces

```
# New module — src/lib/common/clock.zig (exported via the `common` named module)
pub fn nowMillis() i64;   // wall-clock milliseconds since Unix epoch (replaces std.time.milliTimestamp)
pub fn nowNanos() i128;   // wall-clock nanoseconds since Unix epoch (replaces std.time.nanoTimestamp)

# build.zig.zon — pinned revisions (exact SHAs/hashes from the spike branch)
nullclaw  → ref=v2026.5.29
pg        → ref=master#<0.16 commit>   (pinned commit, never a moving branch — existing convention)
posthog   → #v0.2.0
zbench    → ref=zig-0.16.0
httpz     → path=vendor/httpz          (re-vendored zig-0.16 @ 40be022 + UAF patch)
zig_yaml  → path=vendor/zig-yaml       (forked main @ 84d747b, conformance step dropped)

# vendor/httpz/src/worker.zig — non-blocking Worker.deinit MUST contain, before the arena free:
self.thread_pool.stop();
self.thread_pool.deinit();
```

Contract: `clock.nowMillis()`/`nowNanos()` return the SAME epoch semantics as the removed `std.time.*` functions — drop-in replacements, no caller-visible behaviour change. The httpz patch is an invariant, not a suggestion (§Invariants).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| httpz shutdown UAF regression | UAF patch dropped/mis-ported during re-vendor | `make memleak` + integration teardown on Linux non-blocking loop SIGSEGV/leak-reports; CI red before merge |
| Missed timestamp site | A `std.time.*` call not redirected | Build fails (symbol removed) OR orphan sweep `test_no_residual_*` shows >0; gate blocks |
| http.Client missing `io` | A `std.http.Client` construction left as `.{ .allocator = … }` | Build fails on 0.16 (`io` is required, no default); compile error names the site; gate blocks before merge |
| Auth-logic drift in http migration | An auth-boundary edit goes beyond mechanical `io`-threading | §3b STOP-and-surface guard + `/review` over `clerk_backend`/`jwks`; any non-mechanical change halts to Indy with an ack quote |
| Wrong clock epoch/unit | Helper returns monotonic ns or wrong scale | `test_clock_now_millis_is_wall_time` asserts ms magnitude ≈ Unix epoch, not a small monotonic counter |
| zbench branch breaks bench build | `zig-0.16.0` branch incompatible with our bench harness | `zig build bench` compile check (bench lane); fall back to a pinned commit or fork-branch |
| CI image SHA mismatch | Wrong/stale Zig SHA256 in `versions.env` | Image bake fails loud on checksum verify; no green CI until corrected |
| pg.zig master drift | Moving `master` ref breaks mid-PR | Pin the exact commit (not `#master`) per existing zon convention; bump deliberately |
| zig-yaml upstream changes shape | Re-vendor diverges from fork | `CHANGES.md` pins the exact upstream commit; vendored copy is verbatim + documented patch only |

---

## Invariants

1. **No wall-clock site bypasses the helper** — zero `std.time.milliTimestamp`/`nanoTimestamp` in `src/`. Enforced by the orphan-sweep tests (RULE ORP) run in CI, not by review.
2. **httpz UAF patch is present** — non-blocking `Worker.deinit` calls `thread_pool.stop()` before `thread_pool.deinit()`. Enforced by `test_httpz_worker_deinit_stops_pool` (a source/behaviour assertion), not by trusting the re-vendor.
3. **Single Zig-version source of truth** — `build.zig.zon` `minimum_zig_version`, `versions.env` `ZIG_VERSION`, and the CI image tags all read `0.16.0`. Enforced by `test_no_residual_0152_in_ci` + a version-consistency grep (RULE UFS).
4. **Cross-compile parity** — both `x86_64-linux` and `aarch64-linux` build (RULE XCC). Enforced by the cross-compile CI lane.
5. **No milestone IDs in source** — no `M82`/`§`/dim tokens in `.zig`/`.sh`/test names (RULE TST-NAM). Enforced by the milestone-ID audit.
6. **Every `std.http.Client` carries an `io`** — zero `std.http.Client = .{ .allocator = ... }` constructions without `.io` in `src/`. Enforced by the 0.16 compiler (required field) + an orphan grep; the auth-boundary sites change *only* their `io` threading, nothing in the credential/token logic.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_dep_graph_resolves_on_016` | `zig build --fetch` on 0.16 resolves all deps; `zig build` produces `zombied` |
| 1.2 | unit | `test_httpz_worker_deinit_stops_pool` | non-blocking `Worker.deinit` invokes `thread_pool.stop()` before `deinit()` (source/behaviour assertion) |
| 1.3 | unit | `test_yaml_frontmatter_parses_on_016` | a SKILL.md frontmatter fixture parses to expected JSON via the vendored `yaml` module |
| 1.4 | unit | `test_telemetry_init_compiles_with_io` | `preflight` builds + initialises posthog client with the v0.2.0 `io` arg; null-key path still disables cleanly |
| 2.1 | unit | `test_clock_now_millis_is_wall_time` | `nowMillis()` returns a value ≈ current Unix ms (> year-2020 threshold), `nowNanos()` ≈ ms×1e6 |
| 2.2 | unit (orphan) | `test_no_residual_milli_timestamp` | `git grep -c 'std.time.milliTimestamp' src/` == 0 |
| 2.3 | unit (orphan) | `test_no_residual_nano_timestamp` | `git grep -c 'std.time.nanoTimestamp' src/` == 0 |
| 2.4 | unit | `test_log_envelope_timestamp_via_clock` | a log emit carries a wall-clock `ts` field sourced from `clock`; envelope field set unchanged vs 0.15 baseline |
| 3.1 | integration | `test_builds_with_debug_allocator` | `zombied` builds with `DebugAllocator`; `make memleak` clean |
| 3.2 | unit | `test_fixed_writer_emits_expected_bytes` | each migrated `Io.Writer.fixed` site emits byte-identical output to its 0.15 `fixedBufferStream` baseline |
| 3.3 | unit | `test_hashmap_unmanaged_round_trip` | put/get/iterate over the migrated maps returns inserted entries |
| 3.4 | integration | `test_arraylist_016_api_compiles` | the full build compiles with the 0.16 ArrayList API |
| 3b.1 | unit | `test_http_client_fetch_with_io` | `std.http.Client` built with `.io` performs a fetch; `control_plane_client`/`otel_logs` round-trip byte-identical to 0.15 baseline |
| 3b.2 | unit | `test_auth_http_client_io_threaded` | `clerk_backend`/`jwks` build + fetch with threaded `io`; null-key/no-token paths still gate cleanly (no auth-logic change) |
| 3b.3 | integration | `test_integration_http_round_trip_on_016` | full HTTP integration suite green on 0.16 with the threaded `Io.Threaded` |
| 4.1 | integration | `test_ci_images_pin_016` | `versions.env` `ZIG_VERSION==0.16.0`; baked image reports `zig version` 0.16.0 |
| 4.2 | unit (orphan) | `test_no_residual_0152_in_ci` | `grep -rn '0.15.2' .github/workflows playbooks/013_ci_zig_images` == 0 |
| 4.3 | integration | `test_cross_compile_both_linux_targets` | `zig build -Dtarget=x86_64-linux` && `-Dtarget=aarch64-linux` both exit 0 |
| 5.1 | unit (orphan) | `test_rule_zal_references_016` | RULES.md RULE ZAL contains no "0.15" ArrayList reference |
| 6.1 | unit | `test_pool_acquire_immediate_when_free` | capacity=10, active=3 → acquires with no sleep path; active increments to 4 |
| 6.2 ⭐ | unit | `test_pool_acquire_timeout_when_saturated` | capacity=1, active=1, timeout=50ms, no release → returns `AcquireTimeout`, no hang; assert range `50ms ≤ elapsed < 70ms` (never exact timing) |
| 6.3 ⭐ | concurrency | `test_pool_acquire_succeeds_on_release_before_deadline` | capacity=1 saturated, timeout=1s, bg thread releases at 20ms → acquire succeeds, `elapsed < timeout` (the core wake-correctness test under the bounded poll) |
| 6.4 | unit | `test_pool_acquire_deadline_recomputed_each_poll` | timeout=5ms, poll≈2ms, saturated → still times out — guards the "sleep/reacquire without recomputing remaining" bug |
| 6.5 | unit | `test_pool_acquire_repeated_poll_no_state_corruption` | saturated, many sleep/recheck cycles → counts/state intact, no double-count (polling = the spurious-wake equivalent) |
| 6.6 ⭐ | concurrency | `test_pool_one_slot_n_waiters_exactly_one_wins` | capacity=1, 10 waiters, single release → exactly one acquires, others keep waiting (no double-allocation) |
| 6.7 | concurrency | `test_pool_n_waiters_n_releases_no_lost_capacity` | capacity=1, 20 waiters, repeated release/acquire → no deadlock, no lost capacity, active never > capacity |
| 6.8 ⭐ | concurrency | `test_pool_capacity_invariant_under_stress` | many threads acquire/work/release continuously → `0 ≤ active ≤ capacity` holds at all times (the pool's core invariant) |
| 6.9 | integration | `test_pool_failed_dial_does_not_leak_slot` | Redis unreachable during connect → pool healthy; failed connection creation does NOT leak an active slot |
| 6.10 | integration | `test_pool_slow_connect_timeout_correct` | artificial connect delay + many concurrent acquires → timeout behaviour correct, pool does not wedge |
| 6.11 | integration | `test_pool_acquire_release_storm_no_leak` | 1000+ acquire/release cycles → no leaked connections, active returns to 0, shutdown succeeds |
| 6.12 ⭐ | integration | `test_pool_shutdown_with_blocked_waiters` | saturated pool + blocked waiters, then shutdown → waiters exit cleanly, no deadlock, no stuck thread |

- **§6 context:** Zig 0.16 removed `timedWait` from every Io sync primitive (`Mutex`/`Condition`/`Semaphore`). `redis_pool.waitForActiveSlot` switched from `Condition.timedWait` to a **bounded poll** (release mutex → sleep `min(remaining, ~2ms)` via `Clock.Timestamp` → reacquire → re-check predicate + deadline). The `AcquireTimeout` liveness contract is fully preserved; only the wake mechanism degrades from signal-driven to short-poll. `not_full: common.Condition` + its `signal()` calls are kept (no-op while polling) so a future signal-driven `timedWait` slots back in by changing only `waitForActiveSlot`. ⭐ = the five that catch ~90% of production pool failures. Cancellation test (ChatGPT #6) is N/A — the poll-based acquire exposes no cancel seam.
- **Regression:** the migration is behaviour-preserving — the full existing `make test` / `make test-integration` suite is the regression guard; it must pass unchanged on 0.16. Byte-level baselines for logging (2.4) and fixed-writer (3.2) protect the two surfaces where output shape could silently drift.
- **Idempotency/replay:** N/A — no retry semantics introduced.

---

## Acceptance Criteria

- [ ] `zig build` succeeds on Zig 0.16.0 — verify: `zig version | grep 0.16.0 && zig build`
- [ ] No residual removed-API references — verify: `git grep -c 'std.time.\(milli\|nano\)Timestamp' src/` → `0`
- [ ] httpz UAF patch present — verify: `grep -A2 'thread_pool.stop' vendor/httpz/src/worker.zig`
- [ ] No `0.15.2` anywhere in CI — verify: `grep -rn '0.15.2' .github/workflows playbooks/013_ci_zig_images | head`
- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (httpz/HTTP touched)
- [ ] `make memleak` clean (allocator + httpz lifecycle touched) — paste into Verification Evidence / cite CI URL
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] RULE ZAL reconciled to 0.16 in `docs/greptile-learnings/RULES.md`

---

## Eval Commands (post-implementation)

```bash
# E1: Zig is 0.16
zig version | grep -q 0.16.0 && echo "PASS" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -3
# E3: Tests
make test 2>&1 | tail -5
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -2 && zig build -Dtarget=aarch64-linux 2>&1 | tail -2
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Orphan sweeps (empty = pass)
git grep -n 'std\.time\.\(milli\|nano\)Timestamp' src/ | head
grep -rn '0.15.2' .github/workflows playbooks/013_ci_zig_images | head
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no source files deleted; old vendored `httpz` is overwritten in place, not removed | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `std.time.milliTimestamp` | `git grep -n 'std.time.milliTimestamp' src/ \| head` | 0 matches |
| `std.time.nanoTimestamp` | `git grep -n 'std.time.nanoTimestamp' src/ \| head` | 0 matches |
| `std.io.fixedBufferStream` | `git grep -n 'fixedBufferStream' src/ \| head` | 0 matches |
| `GeneralPurposeAllocator` | `git grep -n 'GeneralPurposeAllocator' src/ \| head` | 0 matches |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults and decisions.

- **Spike origin:** feasibility proven on `spike/zig-0.16-feasibility` — full dep graph compiled on 0.16, only `main.zig` failed (3 mechanical renames). httpz UAF patch verified still-required on upstream `zig-0.16`. zig-yaml conformance-harness breakage discovered and forked. These findings are the basis for §1–§3.
- **CTO sequencing call (Jun 02, 2026):** spec authored PENDING, parked behind M80; not pulled into the active stream absent a forcing function (Overview). Rationale: no deadline, real churn, experimental upstream httpz 0.16 → no production risk taken to chase a version number.
- **Prerequisite verification (Jun 03, 2026) — both toolchain prerequisites resolved ahead of implementation:**
  - **zlint compatibility CONFIRMED.** zlint `v0.8.1` (the pin in `lint.yml`) parses Zig 0.16 grammar fine — `--print-ast` over all 575 real 0.16 files (full 0.16 std library recursively + the vendored 0.16 deps) returned **zero parse errors** (negative control verified the test catches real failures). 0.16 is a std-library reform, not a grammar change. No zlint fork or lane-gate needed; keep `ZLINT_VERSION: v0.8.1`. This removes the only out-of-our-control prerequisite.
  - **ci-zig 0.16.0 images BUILT + PUSHED.** All three `ghcr.io/usezombie/ci-zig-{alpine,debian-trixie,ubuntu}:0.16.0` are in GHCR (alpine multi-arch amd64+arm64). `playbooks/013_ci_zig_images/versions.env` bumped to 0.16.0 with authoritative ziglang.org SHAs (`x86_64-linux 70e49664…`, `aarch64-linux ea4b09bf…`) — that edit rides in `stash@{0}` with the rest of the foundation. §4 image-bake work is DONE; the workstream only needs to flip the `:0.15.2`→`:0.16.0` tags in the 9 workflows. Note: GHCR pushes were intermittently flaky (transient `DeadlineExceeded`/`connection reset` on the auth token) — each image took 1–2 retries; the cached layers make retries cheap.
- **Scope reshape — `std.http.Client` pulled in, M82_002 dissolved (Jun 03, 2026):** At CHORE(open), Indy questioned the http.Client deferral. Investigation found 0.16's `std.http.Client` adds a **required, no-default `io: Io` field** (`$ZIG/lib/std/http/Client.zig` — `allocator: Allocator,` + `io: Io,`, neither defaulted). All sites construct `.{ .allocator = alloc }`, which does not compile on 0.16. Acceptance Criterion #1 ("`zig build` succeeds on 0.16") is therefore unsatisfiable while http.Client is deferred — the M82_001/M82_002 split was infeasible, not merely inconvenient. The spike never surfaced this: its diff touches zero http.Client sites, so "full src compiles on 0.16" was never actually true (only the dep graph was proven). Decision: fold the http.Client `io`-threading into B1 (§3b), dissolve M82_002.
  - **Indy directive (verbatim):** _"I want to migrate all to the 0.16.0 approach not cling to 0.15.2"_ — context: whether to keep the conservative split or migrate everything, including `std.http.Client`, to 0.16 in one go. Resolved: migrate all; minimal-`Io` approach (clock via direct syscall, `Io` threaded only to the http.Client sites that require it); full `std.Io` daemon adoption stays out of scope.
- **Q&A banked at CHORE(open) (Jun 03, 2026):** (1) `vendor/httpz` is a verbatim copy of `karlseguin/http.zig` branch `zig-0.16` @ `40be022` — confirmed via `git ls-remote` to be the *current* upstream `zig-0.16` HEAD — plus the one documented UAF stop-before-deinit patch. (2) Post-migration, local `make`/`zig build`/`make test` resolve `zig` via the repo-local `.mise.toml` (→ 0.16.0); `docker-compose` runs stock `postgres:18`/`redis:7` (no Zig) plus a `zombied` container whose `Dockerfile` `COPY`s a *host-built* binary (host build = mise = 0.16.0); CI uses `ci-zig-*:0.16.0`. No 0.15.2 survives once §4 lands.
- **0.16 is a full `std.Io` reform, not mechanical renames (Jun 03, 2026) — scope corrected:** The spike's "only mechanical renames remain" premise was false. 0.16 moved `std.fs`/`std.net`/file-socket I/O and the sync primitives (`Mutex`/`Condition`/`RwLock`) **under `std.Io`**, and the operations **take an `io: Io` param** — `openFile(dir, io, …)`, `file.reader(io, …)`, and critically **`std.Io.Mutex.lock(m, io)`/`unlock(m, io)`**. Empirically (workflow `wf_5e2b0e5e`, 14 agents, verified by grep against the real 0.16 std): the migration **forces `io` into ~80 sites regardless of approach** — 66 mutex lock/unlock/wait calls across 8 files + ~17 runner `std.fs.*Absolute` sites + 5 `http.Client` + 2 `tcpConnectToHost`. The prior "io threaded only to ~5 http.Client sites / minimal-Io" claim was an **undercount** (it predated hitting the `Mutex.lock(io)` requirement) and is hereby corrected. The real *io-needing* surface is ~21 distinct functions (runner 16 + zombied ~5); the rest are pure renames. So the decision was never "thread or not" — it is **visible io vs hidden io** at sites the compiler forces anyway.
- **Architecture decision — Option B (hybrid), made Jun 03, 2026.** Unanimous (5/5 independent lenses: migration-risk, async-substrate, zig-idiom, testability, cognitive-load; 0/3 adversarial overturns). Two external reviews (one pro-global, one adversarial 7-attack) + the workflow converged on B:
  - **zombied (control plane):** `std.Io.Threaded` constructed once in `cmd/serve.zig::run()`, carried as an **`io` field on the existing `http_handler.Context` DI seam** (serve.zig:188-209) — **explicit, not a bare process-global**. Concurrency/network structs (`jwks.Verifier`, `RedisPool`, `sinks`, metrics, redis dial) source `io` from their owning Context, staying unit-testable with a loopback io. zombied's own `std.fs`/`std.process` surface is light: ~2 config fns + 3 http leaves + the JWKS chain; the 51 `getEnvVarOwned` leaves are posix wrappers needing no io.
  - **runner (execution plane):** `io` **threaded signature-only** from `main.zig`→`runLoop(io,…)` down the verified 6-level spine (already threads `alloc`/`cfg` positionally, so `io` rides at near-zero marginal cost), reaching the 16 distinct fns + cgroup/sandbox callees. **Single-lease execution stays sequential — NO async/concurrency refactor in M82.** This plants the seam the async-first scheduler will own without pulling that rewrite in.
  - **Indy gos (verbatim, Jun 03, 2026):** _"1, 2,3,4,5 are go"_ and _"For 1 - Runner io threaded now yes"_ — context: (1) runner io threaded now, signature-only; (2) JWKS lock-across-fetch filed as a separate security-boundary follow-up, not this PR; (3) zombied io as a Context field not a global; (4) vendored deps already 0.16-resolve (no vendoring sub-task); (5) changelog = Session-Notes-only.
  - **Accepted residual (recorded, not an oversight):** zombied keeps the `Io.Threaded` for the config-load + `clerk_backend` + `otel_logs` leaf sites; if zombied ever goes async-first these re-migrate. Knowing, bounded, reversible.
- **JWKS lock-across-fetch — in-scope io-thread, out-of-scope fix (filed):** the `Mutex.lock(io)` rename forces `io` through `verifyAndDecode→lookupKey→refreshCacheLocked→fetchJwksJson` (in scope). It surfaced a real smell — `self.mutex` is held across the blocking `client.fetch()` on the auth hot path (`jwks.zig:175`/`234`). The **fix** (release-lock-before-fetch or per-request/thread-local verifier) is a separate security-boundary spec per project policy, Indy-acked above. Pre-2.0 (RULE NLG): the follow-up uses no "legacy" framing and adds no shim.
- **Runner child-spawn model — `fork`/`pipe`/`close`/`waitpid` removed in 0.16 (Jun 03, 2026):** the runner's manual fork → pipe → dup2 → setpgid → exec child-creation (`child_process.zig`) broke — 0.16 removed `std.posix.{fork,pipe,dup2,setpgid,close,waitpid,exit}` (kept `read`/`poll`/`kill`). The only portable replacement is `std.process.spawn(io, …)` for creation + `process.Child.wait(io)`/`File.close(io)` for lifecycle. CTO review walked the threat model (untrusted/hostile agent): **kill stays cgroup-atomic** (`scope.kill`, whole-tree) — never `Child.kill()` (single-pid → descendant escape); **result read stays a bounded poll/read** on the raw `.handle` (DoS defense); the `process.Child` wrapper is kept ONLY for the `close`/`wait` primitives 0.16 deleted. `.pgid = 0` preserves `setpgid(0,0)` (group-kill fallback) — impl-confirmed at `Threaded.zig:15004`.
  - **Indy gos (verbatim, Jun 03, 2026):** _"yes walk me the model first"_ then _"Yes good to implement"_ — context: rewrite `forkExec` → `process.spawn` with raw-fd/cgroup-centric supervision, after a full model + threat-model walkthrough. Mid-impl forced revision recorded: because `close`/`waitpid` are also gone, the `process.Child` wrapper is retained for lifecycle (not fully dropped as first scoped) — the security core (cgroup kill, bounded read) is unchanged.
  - **Batch 6 VERIFY tests (ChatGPT CTO review, Jun 03, 2026):** the spawn migration adds two correctness tests beyond the existing suite — (a) a bad `argv[0]`/bwrap path makes `process.spawn` return a `SpawnError` **synchronously** (not a child that exits 127), and (b) writing the lease then closing the child's stdin still drives the child to EOF → completion (pipe-ownership regression — the most common spawn-migration failure mode). Land both in the runner suite at Batch 6. The env/fd/argv/kill-tree *hardening* is out of scope here (M84_001).
- **Sandbox-hardening — separate security-boundary follow-up spec (Indy-acked):** the rogue-agent review surfaced three findings that are **pre-existing** (NOT introduced by M82, which is behaviour-preserving): (1) the sandboxed child inherits the daemon environment → possible `ZOMBIE_RUNNER_TOKEN` exfiltration (no bwrap `--clearenv`); (2) non-CLOEXEC daemon fds inherited into the sandbox; (3) `argv[0]`-absolute should be asserted (`spawn` PATH-resolves a relative `argv[0]` via parent env). Filed as its own security spec per the house split rule; pre-2.0 RULE NLG — no "legacy" framing, no shim.
  - **Indy go (verbatim, Jun 03, 2026):** _"ensure the sandbox-hardening followup is done (separate spec)"_ — context: capturing the three adversarial findings as a separate spec, not folded into the toolchain bump.
- **`getEnvVarOwned` is REMOVED in 0.16 — the spec's env blind spot (Jun 04, 2026):** §Discovery's Option-B note claimed *"the 51 `getEnvVarOwned` leaves are posix wrappers needing no io"* — assuming the symbol survives. It does not; 0.16 deleted `std.process.getEnvVarOwned`. The environment is now an immutable `std.process.Environ.Map` snapshot taken at `std.process.Init`; reading env requires *holding* that snapshot. The mechanical migration threaded the snapshot (`env_map: *const Environ.Map`, read via a shared `common.env.owned` facade) to the old scattered read sites — un-specced work that crept in as a shim. This corrects the spec: env threading is real, ~46 zombied prod sites + the test surface.
- **Env-layer consolidation — deliberate scope expansion over "minimal patch" (Jun 04, 2026):** the Decomposition verdict pins this as *"a patch (behaviour-preserving)... the larger refactor is out of scope."* Indy overrode that for the env layer specifically, to retire the `env_map` shim rather than ship it. Result (both binaries exe-green): handler auth secrets (`CLERK_WEBHOOK_SECRET`/`APPROVAL_SIGNING_SECRET`/`CLERK_SECRET_KEY`) resolved ONCE at boot into typed `http_handler.Context` fields (handlers borrow read-only — no per-request env read); `Context.env_map` removed; SSE subscriber dials from the pool's resolved config via new `redis_subscriber.connectFromConfig` (no env); `jwks_env_var` (dead — set by nothing) removed along with `env_map` from jwks/oidc. **`env_map` now lives ONLY in the boot/config-resolution layer** (serve/doctor/migrate entry + `ServeConfig.load` + db/redis/otel/balance connect reads).
  - **Indy directive (verbatim, Jun 04, 2026):** _"Would we not collect it at the start and just pass a context object or so? meaning when the program start parse the envs and stick it to a struct and then pass that struct?"_ and _"I want C and the broader cleaner up where you did the shim by passing env_map cleaned up in this PR. Ensure its fixed and pushed in this PR."_
- **KEK = Option C (boot-resolved), Option A rejected (Jun 04, 2026):** `crypto_primitives.loadKek` re-read `ENCRYPTION_MASTER_KEY` from env on every envelope encrypt/decrypt, although `ServeConfig` already loads + validates it at boot — a duplicate read whose threading (Option A) would drag the env reader through `crypto_store`→`vault`→`serve_webhook_lookup`→the **webhook auth middleware** (AUTH gate fires). Decision **C**: `crypto_primitives.setKekFromHex()` decodes the validated config value into a process-`var g_kek` once in `serve.run`; `loadKek()` returns it — no alloc, no env, no auth-flow. Rejected A as a leaky abstraction crossing the security boundary for a boot-immutable value.
  - **Indy decision (verbatim, Jun 04, 2026):** _"I want C"_ — context: KEK threading, A (thread `env_map` through the crypto/auth chain) vs C (resolve once at boot from `ServeConfig`).
- **CTO shim-hack review (Jun 04, 2026):** measured against Option B's north star (*"io as an explicit Context field — not a bare process-global... unit-testable with a loopback io"*), three shims were identified: **(1)** `env_map`-everywhere [FIXED — consolidation above]; **(2)** the `loadKek` KEK double-read [FIXED — Option C]; **(3)** `globalIo()` standing in for the Context-threaded `io` the spec mandates — acceptable for ambient primitives (mutex/condition/sleep/CSPRNG, which are file-scope globals) and Context-less background HTTP (otel/posthog flush threads), but the **jwks + clerk `std.http.Client`** sites should source `io` from their owning Context per §3b. Shim #3 is **surfaced, NOT in this PR's env scope** — Indy to decide fold-in vs separate follow-up; recorded here so it is not lost.
- **Pickup VERIFY + close (Jun 04, 2026):** the handoff's "remaining blocker" (2 `runner_register` operator-CLI fails) was **misdiagnosed** as a `MultiReader`/`awaitConcurrent` bug. Real cause: `common.globalIo()` → `std.Io.Threaded.global_single_threaded`, whose `.allocator = .failing`, so `std.process.run`/`spawn` OOMs at the pre-fork argv-buffer allocation for **any** child (even `echo`; isolated with a standalone repro). Production is unaffected — it spawns via `init.io`; `globalIo()` is the deliberate non-spawning blocking seam. Fix: tests spawn through a real `std.Io.Threaded.init(alloc, .{})` io (`runCli`/`runProc`). The redis restart-reconnect test (`redis_pool_test.zig` #24) was silently **skip-masked** by the same bug (`std.process.run(...) catch return error.SkipZigTest` at its docker guard) — now runs (container restart + reconnect verified). Bench tooling (`tests/bench/`) was unmigrated, and `make bench` was already broken on **main** via a missing `auth_codes` dep in the `bench_app` bridge — migrated to 0.16 + dep repaired; same-machine 0.15.2-vs-0.16.0 baseline pinned in `tests/bench/benchmark.md` (0.16 faster on every path).
- **Follow-ups surfaced (separate specs, NOT this PR):** (1) vendor/pg `acquire` uses `Io.Condition.waitUncancelable` — per-acquire timeout dropped → indefinite block under pool exhaustion (mitigated by conn-level statement/read timeouts); restore a bounded wait via the redis pool's bounded-poll pattern (§6). (2) `generateExecutionId` (`engine/types.zig`) degrades to an all-zero id on CSPRNG failure → collision risk; fail-closed instead. (3) `make/build.mk` alpine static-build still fetches zig 0.15.2 (outside the spec's CI orphan-sweep scope). (4) Widen the orphan sweep beyond `src/` to catch `tests/bench/` flag-gated code.
- **Consults** — {Architecture / Legacy-Design / gate-flag triage: question + Indy's decision, as they arise.}
- **Skill chain outcomes** — `/write-unit-test`: pickup diff is test/bench tooling — no new production surface; ledger 6/6 resolved; red-green proven (globalIo spawn OOM → Threaded.init pass; suite 26/2 → 28/0). `/review`: clean — no findings on the session diff. `/security-review`: cert `.awake→.real` CLEAN (restores wall-clock cert validation); vendor/pg acquire + `generateExecutionId` = pre-existing follow-ups (above). `/review-pr` + `kishore-babysit-prs`: post-PR.
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here.

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification (orphan sweeps, byte-baselines, memleak). | Clean. Iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/ZIG_RULES.md`, Failure Modes, Invariants (esp. httpz patch + orphan completeness). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration suite (clean DB+Redis) | `make test-integration` | **1467 pass · 6 skip · 0 fail · 0 leaks** (34/34 steps). 2 runner-enrollment operator-CLI tests fixed; redis restart-reconnect test activated (was skip-masked) | ✅ |
| Memleak | `make memleak` | allocator-guard test + macOS `leaks` gate pass; 0 leaks across 1467 tests | ✅ |
| Lint | `make lint-zig` | clean (zig fmt + zlint + FLL + pg-drain + role/legacy guards + orphan sweep) | ✅ |
| Cross-compile (RULE XCC) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` (both binaries) | zombied + zombie-runner × {x86_64, aarch64} — 4/4 green | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (2384 commits scanned) | ✅ |
| Timestamp orphan sweep | `git grep -n 'std.time.\(milli\|nano\)Timestamp' src/` | 0 matches (tests/bench also clean post-migration) | ✅ |
| Bench (Tier-1 micro + Redis concurrency) | `make _bench-micro` · `make bench-redis` | 0.16.0 faster than 0.15.2 on every path (json −38%, chunk −37%, redis throughput +17.8%); same-machine baseline pinned in `tests/bench/benchmark.md` | ✅ |

---

## Out of Scope

- ~~**`std.http.Client` rewrite → M82_002 (B2).**~~ **Pulled in-scope (§3b), M82_002 dissolved** — 0.16's required `io: Io` field makes deferral infeasible (the build won't compile without it). The 5 sites (`clerk_backend`, `jwks`, `control_plane_client`, runner loop caller, `otel_logs`) are migrated here.
- **JWKS lock-held-across-network-fetch FIX → separate security-boundary follow-up spec (Indy-acked).** `jwks.zig` holds `self.mutex` across the blocking `client.fetch()` on the auth hot path (`:175`/`:234`). M82 *threads `io`* through that chain (forced by the `Mutex.lock(io)` rename) but does **not** change the lock semantics. The fix (release-lock-before-fetch or per-request/thread-local verifier) is its own spec+PR; pre-2.0 RULE NLG — no "legacy" framing, no shim.
- **Full `std.Io` async adoption** (concurrent lease execution, `io.async`, io_uring backend, `Io.EventLoop`) — the future async-scheduler milestone. M82 threads `io` signature-only in the runner (sets the seam) but keeps single-lease sequential execution; clock stays a direct syscall. Not triggered by toolchain currency.
- **Dropping the `httpz`/`zig-yaml` vendors** once upstream lands 0.16 fixes — a future re-pin; tracked in each `vendor/*/CHANGES.md`.
- **Bumping `VERSION` / changelog** — internal toolchain change, no user-visible behaviour; no `<Update>` entry.
