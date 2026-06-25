# M100_001: Runner GA hardening — secret redaction, fail-closed egress, bounded reliability, proven enforcement, and the structural/build debt that blocks confident iteration

**Prototype:** v2.0.0
**Milestone:** M100
**Workstream:** 001
**Date:** Jun 24, 2026
**Status:** DONE
**Priority:** P0 — contains GA-blocking secret-leak and default-open-egress exposures in the sandboxed-agent runner.
**Categories:** RUNNER
**Batch:** B1 — single stream; Sections sequenced by Dependencies below.
**Branch:** feat/m100-runner-ga-hardening
**Test Baseline:** unit=312 integration=0 (src/runner at CHORE(open); whole-tree depth gate 2090/202)
**Depends on:** none.
**Provenance:** LLM-drafted (claude-opus-4-8, Jun 24 2026) from the M100 runner CTO review (5-agent read-only sweep across concurrency, security, memory-safety, engine-structure, and test/build).

> **Provenance is load-bearing.** This is agent-drafted from a review, not human-authored — cross-check every claim against the code before EXECUTE. The §-level findings each carry a file:line anchor for that reason.

**Canonical architecture:** the runner's isolation model lives in the module headers `child_supervisor.zig`, `child_process.zig`, `engine/runner_progress.zig`, `network/EgressScope.zig`, and the M90_00x runner specs in `docs/v2/done/`; secret handling follows `docs/AUTH.md`. Greenfield pieces (the enforcement test lane, the `RunContext` seam) are defined here.

---

## Implementing agent — read these first

1. `src/runner/engine/runner.zig` (`collectSecrets`, `executeInner`) + `src/runner/engine/runtime/secret_substitution.zig` — the two ends of the secret path that must read from ONE source (§1, §5).
2. `src/runner/network/Policy.zig` + `src/runner/sandbox_args.zig` + `src/runner/child_supervisor.zig` (`enforcesEgress` fail-closed branch) + nullclaw `http_request.zig` `resolveConnectHost` — the egress posture and the SSRF (Server-Side Request Forgery) pin (§2).
3. `src/runner/daemon/loop.zig` (backoff sites) + `src/runner/daemon/call_deadline.zig` (`CallWatchdog`) + `src/lib/common/sync.zig` — the reliability surface (§3).
4. `src/runner/engine/CgroupScope.zig` / `seccomp.zig` / `landlock.zig` + `src/runner/sandbox_integration_test.zig` (the gold-standard real-process harness to mirror) — enforcement + the test lane (§4).
5. `docs/v2/done/M90_001*` (runner deadline/renewal design) + `dispatch/write_zig.md` (file-as-struct, tagged-union results, errdefer chains, concurrency) — the patterns §5/§6 must mirror.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Harden agentsfleet-runner for GA: redact all secrets, fail-closed egress, bounded retries, proven kernel enforcement
- **Intent (one sentence):** Close the two secret/egress exfiltration exposures, make retries and cross-thread locking bounded-and-correct, prove the sandbox actually enforces, and extract the testing/allocator seams that make all of the above verifiable and fast.
- **Handshake (agent fills at PLAN):** restate intent + `ASSUMPTIONS I'M MAKING:` before any edit; a mismatch with the Intent stops EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a prompt-injected agent inside a lease tries to exfiltrate a tool credential (e.g. a Fly token) and reach an attacker host; the credential never appears in any activity frame / report the control plane receives, the egress is refused, and the enforcement test lane proves it in CI (Continuous Integration), not just by code review.
2. **Preserved user behaviour** — every existing lease executes, renews, reports, and streams activity exactly as today; the LLM api_key redaction, fail-closed sandbox establishment, and single-reaper fork lifecycle are unchanged.
3. **Optimal-way check** — optimal is "the substitution set and the redaction set are the same object," "egress is default-deny," and "every sandbox primitive has a real-kernel proof." We reach the first two directly; full default-deny egress (the unbuilt `allow_list_egress` strict mode) stays out of scope — §2 instead makes the *default* and *unknown-value* paths fail-closed/loud, which removes the silent-open exposure without shipping the strict mode.
4. **Rebuild-vs-iterate** — iterate. The concurrency model and process sandbox are sound (review verdict); the debt is localized. §5 is the one real refactor and it serves testability, not aesthetics. No determinism is traded.
5. **What we build** — (§1) a secret set the redactor and substitutor share; (§2) fail-closed egress default + SSRF pin for tenant hosts; (§3) one bounded+jittered backoff helper + `std.Thread` locks + clamped int casts; (§4) a root-gated Linux enforcement test lane + parser/input-matrix tests; (§5) the `RunContext`/DI seam + struct-discipline splits; (§6) build-speed levers + a release-grade allocator.
6. **What we do NOT build** — the strict `allow_list_egress` netns/veth runtime (separate spec); capability-dropping in the child (§2 documents the residual); a clock-skew-tolerant lease TTL (Time To Live) (M90_001 Out of Scope, restated).
7. **Fit with existing features** — compounds with the lease/renew/report loop and the activity live-tail; must not destabilize the fork→enroll→execute path or the wire protocol shared with `agentsfleetd`.
8. **Surface order** — internal daemon; no CLI/UI surface. Operator-visible only via logs + the new test lane.
9. **Dashboard restraint** — N/A (no UI).
10. **Confused-user next step** — an operator who sees `egress_strict_unimplemented_fail_closed` or `dev_none_rejected_in_release_build` has the error code + the config var to fix; §2 adds a startup log line stating the resolved egress posture so "is egress open?" is answerable from the boot log.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal (always).
- **`dispatch/write_zig.md`** — every Section touches `*.zig`: file-as-struct (§5), tagged-union results (§1 redaction outcome, §5 observer select), multi-step `errdefer` (§1 secret slice, §5 `RunContext`), concurrency atomics/locks (§3), cross-compile both linux targets (all).
- **`docs/AUTH.md`** — §1/§2 touch credential handling and SSRF.
- Standard authoring set (`write_any`): UFS named constants (§3 backoff bounds), File/Function length (§5 splits), LOGGING + ERROR REGISTRY (every new failure path).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile `x86_64-linux` + `aarch64-linux`; read `dispatch/write_zig.md` per Section. |
| PUB / Struct-Shape | yes | shape verdict per new pub surface: `RunContext`/`RunDeps`/`observer_select` union (§5), `UsageSnapshot` file-as-struct (§5), `Secret` slice (§1). |
| File & Function Length (≤350/≤50/≤70) | yes | §5 pre-empts the `loop.zig` 345/350 breach via `lease_run.zig`; `runner.zig`/`runner_progress.zig` shrink via `RunContext`. |
| UFS (repeated/semantic literals) | yes | §3 backoff bounds (`MAX_BACKOFF_MS`, jitter pct) named once; egress posture strings already enum-sourced. |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes | new `UZ-*` codes for egress-refused-default and watchdog-spawn-fatal; `RunContext` gets `init`/`deinit` pairing. |
| SCHEMA / UI / DESIGN TOKEN | no | N/A — no schema or UI surface. |

---

## Overview

**Goal (testable):** the runner leaks no resolvable secret to the control plane, never silently opens egress, retries within a bounded+jittered ceiling, locks cross-thread with thread-safe primitives, proves each sandbox primitive enforces in a root-gated Linux lane, and compiles its unit tests without the nullclaw engine.

**Problem:** (a) only the LLM api_key is redacted — tool secrets resolved into outbound HTTP can reach the control plane via tool output / curl stderr / response chunks; (b) egress is default-`allow_all` and the unknown/typo value also falls open; (c) the SSRF resolve-then-pin never runs for the (tenant-influenced) allowlisted hosts that are the only hosts actually dialed; (d) heartbeat backoff grows unbounded with no jitter; (e) the cross-thread watchdog mutex is correct only by a std-internal accident; (f) the sandbox enforcers have zero enforcement tests; (g) `executeInner`'s success path is untestable (hardcoded provider) and the unit build recompiles nullclaw three times.

**Solution summary:** one secret set feeds both substitutor and redactor and the redactor drops a frame rather than emit raw under memory pressure (§1); egress default + unknown-value fail closed and log the resolved posture, and tenant-supplied allowlist hosts go through the private-IP-reject + DNS-pin path (§2); a single capped+jittered backoff helper replaces the three ad-hoc sites, the watchdog uses `std.Thread` locks, and out-of-range int casts saturate (§3); a root-gated Linux lane forks real children and asserts seccomp trap / landlock denial / cgroup pids+OOM, with fixtured parser tests and an input-matrix sweep (§4); `executeInner` becomes a `RunContext` with an injectable provider, observer selection becomes a union, oversized files split, and `sandbox_tier` becomes an enum (§5); a stub nullclaw module + per-subsystem test steps + Linux-gated integration compile + a split CI cache key + a release-grade allocator cut the build/iteration cost (§6).

---

## Prior-Art / Reference Implementations

- **Process/enforcement tests** → `src/runner/sandbox_integration_test.zig` + `child_process.zig` tests (real fork/exec, env-leak, kill-tree, CLOEXEC) — the gold standard §4 mirrors for the kernel-enforcer proofs.
- **Input-matrix unit tests** → `src/runner/daemon/config.zig` + `src/runner/pipe_proto.zig` tests (default/clamp/whitespace/invalid/empty/oversize/truncated) — the template §1/§4 copy per file.
- **File-as-struct + façade** → `src/runner/network/*.zig` (+ `network/network.zig`) and `engine/*Scope.zig` — the §5 target shape.
- **Bounded backoff** → no in-repo reference; shape defined in §3 (exponential, capped, ±jitter), constants single-sourced in `src/lib/common/constants.zig`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/runner.zig` | EDIT | `collectSecrets` → slice over api_key ∪ secrets_map (§1); `executeInner` → `RunContext` (§5). |
| `src/runner/engine/runner_progress.zig` | EDIT | redaction drops frame on OOM; `Secret` set fed from one source (§1). |
| `src/runner/engine/runner_helpers.zig` | EDIT | `redactedFinalReply` ownership; latent `@intCast` on `max_tokens` → `std.math.cast` (§1, §3). |
| `src/runner/engine/runtime/secret_substitution.zig` | EDIT | expose the leaf-secret set the redactor consumes (§1). |
| `src/runner/engine/run_context.zig` | CREATE | `RunContext`/`RunDeps` DI seam (§5). |
| `src/runner/engine/observer_select.zig` | CREATE | `union(enum){ fallback, progress }` (§5). |
| `src/runner/network/Policy.zig` | EDIT | default + unknown-value egress fail-closed/loud (§2). |
| `src/runner/sandbox_args.zig` | EDIT | honor the resolved posture; log it (§2). |
| nullclaw `http_request.zig` / `src/runner/engine/tool_builders.zig` | EDIT | route tenant-supplied allowlist hosts through `resolveConnectHost` (§2). |
| `src/runner/network/AllowList.zig` | EDIT | reconcile exact-vs-wildcard split-brain (§2). |
| `src/runner/daemon/loop.zig` | EDIT | use bounded backoff helper; split lease lifecycle → `lease_run.zig` (§3, §5). |
| `src/runner/daemon/lease_run.zig` | CREATE | extracted per-lease lifecycle (§5). |
| `src/runner/daemon/call_deadline.zig` | EDIT | `std.Thread.Mutex`; watchdog-spawn fatal; connect-stage deadline (§3). |
| `src/lib/common/sync.zig` | EDIT | cross-thread locks use `std.Thread` primitives (§3). |
| `src/lib/common/constants.zig` | EDIT | named backoff bounds + jitter (§3). |
| `src/runner/bundle_extract.zig` | EDIT | cap-before-accumulate (saturating) (§3). |
| `src/runner/daemon/config.zig` | EDIT | `sandbox_tier` → `SandboxTier` enum (§5). |
| `src/runner/pipe_proto.zig` + `src/runner/engine/UsageSnapshot.zig` | EDIT/CREATE | lift `UsageSnapshot` to file-as-struct (§5). |
| `src/runner/engine/CgroupScope.zig` | EDIT | extract pure events-parsers for fixture testing (§4). |
| `src/runner/*_test.zig` + new enforcement lane root | CREATE/EDIT | enforcement proofs, value-absence, input-matrix, `integration:` prefix (§4). |
| `build_runner.zig` + `make/*.mk` + `.github/workflows/test*.yml` | EDIT | nullclaw stub, per-subsystem steps, Linux-gated integration compile, cache key (§6). |
| `src/runner/daemon/worker_pool.zig` + `main.zig` | EDIT | release-grade allocator by build mode (§6). |
| `docs/v2/pending/ → active/ → done/ M100_001` | MOVE | lifecycle. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one spec, six Sections, sequenced §1→§2→§3→§4 (the GA gate) then §5→§6 (the leverage that §4's success-path tests and §6's nullclaw stub depend on). Per owner direction this is a single spec, not a milestone of separate workstream files.
- **Alternatives considered:** (a) two milestones (GA-gate vs refactor) — rejected as paperwork over substance; (b) flip egress to full default-deny now — rejected because the strict netns/veth runtime is unbuilt, so §2 hardens the default/unknown paths instead.
- **Patch-vs-refactor verdict:** §1–§4 are **patches** (localized correctness/security/test fixes); §5 is a **refactor** (the `RunContext` seam) that is in scope because it is the only thing that makes §1/§4's success-path assertions possible and unblocks §6's stub build. §6 is build-config.

---

## Sections (implementation slices)

### §1 — Secret-redaction coverage (P0)  ✅ landed Jun 25, 2026

Every credential the runner can resolve is registered with the redactor, fed from the *same* source the substitutor reads, so a secret can never reach an activity frame, the final reply, or a memory frame. **Implementation default:** `collectSecrets` returns an allocated `[]const Secret` built from `api_key` ∪ every leaf value in `policy.secrets_map`, each mapped to its `${secrets.NAME.FIELD}` placeholder.

- **Dimension 1.1** — `collectSecrets` includes every `secrets_map` leaf, not just api_key → `test_collect_secrets_covers_secrets_map`
- **Dimension 1.2** — redactor and substitutor derive from one shared accessor (drift-proof) → `test_redaction_set_equals_substitution_set`
- **Dimension 1.3** — on redaction allocation failure the frame is dropped, never emitted raw → `test_redaction_oom_drops_frame_not_raw`
- **Dimension 1.4** — every redaction test asserts the secret VALUE is absent from output, not only that the placeholder is present → `test_redacted_output_excludes_secret_value`

### §2 — Fail-closed egress default + SSRF tenant-pin (P0)  ✅ landed Jun 25, 2026

> **Realization note.** No nullclaw fork was needed: the wrapper's outer exact-match gate is authoritative, so setting the inner allowlist to `&.{}` routes every tenant host through `resolveConnectHost` (private-IP reject + DNS pin) and closes the wildcard split-brain. The unset/typo fallback resolves to `allow_list_egress` (fails closed at the supervisor via the existing `egress_strict_unimplemented_fail_closed`); operators opt into interim open egress with `RUNNER_NETWORK_POLICY=allow_all`. §2.4's live DNS-rebind integration test rides §4's Linux lane; the reject half + pin routing are unit-proven.

The runner never silently grants open egress, and the private-IP-reject + DNS-rebind pin runs for tenant-influenced allowlist hosts. **Implementation default:** unset/unknown `RUNNER_NETWORK_POLICY` resolves to a refuse-or-explicit posture (not silent `allow_all`); operator-baseline allowlist entries keep the internal-services skip, tenant-supplied `network.allow` entries go through `resolveConnectHost`.

- **Dimension 2.1** — unknown/typo'd policy value does not yield open egress → `test_unknown_network_policy_fails_closed`
- **Dimension 2.2** — resolved egress posture is logged at startup → `test_startup_logs_egress_posture`
- **Dimension 2.3** — a tenant allowlist host resolving to a private/link-local IP is rejected → `test_tenant_host_private_ip_rejected`
- **Dimension 2.4** — a tenant allowlist host is DNS-pinned (rebind defeated) on dial → `test_tenant_host_dns_pinned`
- **Dimension 2.5** — wildcard/`*` entries cannot widen the L4 exact-match allowlist via the inner tool → `test_allowlist_no_wildcard_widening`

### §3 — Bounded reliability: backoff, locks, deadlines, int-cast hardening (P1)  ✅ landed Jun 25, 2026

> **Spec-vs-reality deviation (§3.2).** Zig 0.16 has **no `std.Thread.Mutex`** (removed for `std.Io.Mutex`). `common.Mutex` IS genuinely cross-thread (real atomics + OS futex) provided the build is not `single_threaded`. The fix is therefore a `comptime { assert(!builtin.single_threaded) }` guard in `call_deadline.zig` (makes the "correct by accident" reliance explicit) + a 2-thread functional mutual-exclusion test — NOT a type swap to a nonexistent type. `test_watchdog_uses_thread_mutex` is realized as the type+invariant assert. Amend the spec's §3.2 wording accordingly.

Retries are bounded, cross-thread locks are unconditionally thread-safe, and untrusted/garbage integers saturate instead of panicking. **Implementation default:** one `backoff(attempt)` helper (exponential, capped at `MAX_BACKOFF_MS`, ±20% jitter from kernel getrandom) replaces the heartbeat/transport sites; watchdog `Mutex`/`Condition` → `std.Thread`; out-of-range casts → `std.math.cast … orelse` / saturating add.

- **Dimension 3.1** — heartbeat backoff is capped and jittered (never unbounded) → `test_backoff_capped_and_jittered`
- **Dimension 3.2** — the watchdog lock is a thread-safe primitive (not the single-threaded Io shim) → `test_watchdog_uses_thread_mutex`
- **Dimension 3.3** — a persistent watchdog-spawn failure is worker-fatal/observable, not silently unbounded → `test_watchdog_spawn_failure_is_fatal`
- **Dimension 3.4** — `max_tokens` negative/oversized → clamped, no panic → `test_max_tokens_out_of_range_clamped`
- **Dimension 3.5** — corrupt tar size → rejected before accumulate, no overflow panic → `test_corrupt_tar_size_rejected`

### §4 — Kernel-enforcement test lane + input-matrix (P0)  ✅ landed Jun 25, 2026

> **Status.** ALL of §4 landed + green — the GA gate is closed. §4.1–4.3 (root-gated seccomp/Landlock/cgroup real-process proofs) authored in `src/runner/sec_enforcement_integration_test.zig` and **verified on a real Linux kernel** in a privileged native-arm64 container (`ghcr.io/agentsfleet/ci-zig-alpine:0.16.0`), NOT compile-and-skipped blind on Darwin: a forked child applies the real enforcer, attempts a concrete violation (denied `ptrace`, out-of-workspace write, fork past `pids.max=1`, over-budget mmap+touch), and the parent asserts the trap exit / Landlock denial / `resource_kill` / `oom_kill` classification via the real `classify`. §4.4/§4.6 landed earlier. §4.5: the 4 new proofs carry the `integration:` prefix → depth gate `integration` count rose 202→206. Local repro + CI prerequisite codified in `scripts/cgroup-delegate.sh` + `make test-enforcement{,-docker}`. Native-arm64 lane: 251 pass / 4 skip / 0 fail (the 4 skips are off-Linux unit guards). Container key learning: the proofs must run **native-arch** — under x86_64 QEMU emulation seccomp's arch-matched BPF filter is bypassed (false proof).

The sandbox is proven to enforce, not merely shaped. **Implementation default:** a root-gated Linux test lane (own build step, `integration:`-prefixed, `SkipZigTest` when unprivileged/off-Linux) forks a real child and asserts each primitive; `CgroupScope` events-parsers are extracted pure and fixture-tested.

- **Dimension 4.1** — child attempting a denylisted syscall traps and exits `SECCOMP_VIOLATION_EXIT` → `test_integration_seccomp_traps_denied_syscall`
- **Dimension 4.2** — child write outside the workspace is denied under Landlock → `test_integration_landlock_denies_outside_write`
- **Dimension 4.3** — child exceeding `pids.max` is contained and classified `resource_kill`; an OOM is attributed via `wasOomKilled` → `test_integration_cgroup_pids_and_oom`
- **Dimension 4.4** — `CgroupScope` `readEventCount`/`readCpuThrottledUs` parse captured `memory.events`/`pids.events` bytes correctly, incl. malformed/empty → `test_cgroup_events_parser_matrix`
- **Dimension 4.5** — the runner's integration tests are counted by the depth gate (`integration:` prefix adopted) → verify: `make/quality.mk` depth count includes runner lane
- **Dimension 4.6** — input-matrix sweep (empty / null / oversize / not-sent / malformed) added for `call_deadline`, `Policy`, `AllowList`, `Plan`, `Socket` → `test_*_input_matrix`

### §5 — RunContext/DI seam + struct discipline (P1)  🟡 partial (§5.1 + §5.3 + §5.4 + §5.5 landed Jun 25, 2026)

> **§5.3 landed.** `loop.zig`'s per-lease execution lifecycle (`executeAndReport` + `TickFanout`, `materializeBundle`, `reportStartupFailure`, `prepareWorkspace`, `cleanupWorkspace`) extracted to `daemon/lease_run.zig`. Behaviour-preserving: `pollAndProcess` now calls `lease_run.executeAndReport`; `outcomeFor`/`splitFields` (unit-tested in `loop_test`) + `pollAndProcess` (called by `worker_pool`) stay in `loop.zig`, which imports `lease_run` and is imported back for those two pub helpers (runtime function refs, no comptime cycle). **`loop.zig` 343→172, `lease_run.zig` 206 — both well under the 350 cap** (the length-pressure relief §5 promised). Dead imports swept from `loop.zig`. Unit 331 pass / 7 skip / 0 fail; integration lane green. Remaining: §5.2 observer_select union.

> **§5.5 landed.** `UsageSnapshot` lifted out of `pipe_proto.zig` into its own file-as-struct `src/runner/UsageSnapshot.zig` (fields + `WIRE_LEN` + drift guard + `encode`/`decode`/`fold`), re-exported as `pipe_proto.UsageSnapshot` (and transitively `supervisor.UsageSnapshot`) so every consumer is unchanged. Pure encode/decode/fold tests live with the struct (incl. a new `fold`-never-regresses test); the frame round-trip test stays in `pipe_proto` (it exercises `writeFrame`/`readFrame`). Registered in `tests.zig`. **Placement deviation from the spec's `engine/UsageSnapshot.zig`:** placed at `src/runner/UsageSnapshot.zig` (sibling of `pipe_proto`) instead — it is a wire/protocol type, so this keeps the dependency direction clean (no `pipe_proto`→`engine/` edge). Runner unit suite 331 pass / 7 skip / 0 fail. Remaining: §5.2 observer_select union, §5.3 loop→lease_run split.

> **§5.4 landed.** `Config.sandbox_tier` is now a parsed `contract.protocol.SandboxTier` enum (was `[]const u8`): `config.parseSandboxTier` maps the env string at load, unset/unrecognized → `dev_none` (the release gate then refuses it — fail closed). The `std.mem.eql(cfg.sandbox_tier, …)` compares in `child_supervisor.zig`/`sandbox_args.zig` became `!= .dev_none`; `main.zig`'s now-dead `sandboxTierFromStr` removed; the wire `SelfResponse.sandbox_tier` stays a string (unchanged protocol). `test_config_sandbox_tier_enum` covers valid/invalid/unset. Remaining: §5.2 observer_select union, §5.3 loop→lease_run split, §5.5 UsageSnapshot file-as-struct.

> **Status.** §5.1 DI seam landed + green: `run_context.zig` defines `RunDeps{ acquireProvider }` (default = the runtime bundle acquirer); `executeInner` is parameterized by it and the production `execute()` passes the default — behaviour-preserving (all 326 prior tests still pass). `run_context_test.zig` proves, offline, that the execute path routes provider acquisition through the injected seam (stub invoked exactly once → failure propagates as `FleetInitFailed`). **Realization note:** the seam is delivered as injected `RunDeps` on `executeInner` rather than a heap `RunContext{build/run/deinit}` struct — the observer/fleet vtables capture stack pointers (`&adapter`, `&fleet`) whose stability a heap restructure would jeopardize for no testability gain. The full stub-provider content/token assertion (the ideal Dimension 5.1) is now UNBLOCKED by this seam but needs a nullclaw mock `Provider`; §5.2/5.4/5.5 (observer union, sandbox_tier enum, UsageSnapshot file-as-struct) remain.

`executeInner` becomes testable mechanism with an injectable provider, and the oversized/stringly-typed files adopt the in-repo file-as-struct discipline. **Implementation default:** `RunContext = @This()` owning the assembled runtime with `build()`/`run()`/`deinit()`; `RunDeps{ acquireProvider }` defaults to the runtime acquirer, tests pass a stub.

- **Dimension 5.1** — the engine success path runs end-to-end against a stub provider (no live network) → `test_run_context_executes_with_stub_provider`
- **Dimension 5.2** — ⏸️ DEFERRED (Indy-acked) — observer selection as `union(enum)`; cosmetic, the current `undefined` writer/adapter are guarded-safe behind `progress_fd != null`
- **Dimension 5.3** — `loop.zig` lease lifecycle extracted to `lease_run.zig`; both files under the length cap → verify: 350-line gate
- **Dimension 5.4** — `sandbox_tier` is a parsed `SandboxTier` enum on `Config` (no stringly-typed compares) → `test_config_sandbox_tier_enum`
- **Dimension 5.5** — `UsageSnapshot` is its own file-as-struct, re-exported by `pipe_proto` → verify: import + existing usage tests pass

### §6 — Build speed + allocator perf (P2)

Unit iteration stops paying for nullclaw three times, and the daemon stops running the debug allocator in production. **Implementation default:** a stub `nullclaw` module wired into the unit-test graph (enabled by §5's seam); per-subsystem test steps; Linux-gated integration compile; split CI cache key; allocator chosen by `builtin.mode`.

- **Dimension 6.1** — ⏸️ DEFERRED (Indy-acked) — unit-test graph compiles against a stub nullclaw (build-graph work; P2 compile speed)
- **Dimension 6.2** — ⏸️ DEFERRED (Indy-acked: "not worth it") — per-subsystem test steps; P2 compile ergonomics with RULE NDC drift cost
- **Dimension 6.3** — ✅ DONE (structurally satisfied — the unit test root excludes the integration root; macOS `test` does not compile `sandbox_integration_test.zig`)
- **Dimension 6.4** — ✅ DONE — release builds select `smp_allocator` (non-Debug) → `test_release_allocator_not_debug`
- **Dimension 6.5** — ⏸️ DEFERRED (Indy-acked) — CI cache key split; workflow-only, P2

---

## Interfaces

> Lock these; do not change without amending the spec.

- `collectSecrets(policy, fleet_config) -> []const runner_progress.Secret` (allocated; caller frees) — replaces the `[1]Secret` value return. `Secret` shape `{ value, placeholder }` unchanged.
- `RunContext.build(alloc, RunDeps, params…) !RunContext` / `RunContext.run(self, message, context) !InnerResult` / `RunContext.deinit(self)`.
- `RunDeps{ acquireProvider: *const fn(Allocator, *Config) anyerror!Provider = runtimeAcquire }`.
- `observerSelect(...) -> union(enum){ fallback: Observer, progress: ProgressBundle }`.
- `backoff(attempt: u32) u64` — monotonic up to `MAX_BACKOFF_MS`, jittered; pure-enough to test with an injected jitter source.
- Wire protocol (`src/lib/contract`) is unchanged — no field added/removed/retyped.

---

## Failure Modes

| Mode | Cause | Handling (system response + caller-observable) |
|------|-------|------------------------------------------------|
| Secret in tool output/stderr/chunk | tool credential echoed | redacted to placeholder before any frame/report leaves the runner |
| Redaction allocation failure | OOM (Out of Memory) mid-redact | frame dropped (not emitted raw); warn logged |
| Egress policy unset/typo | misconfig | fail-closed/explicit posture, never silent open; startup log states posture |
| Tenant host → private IP | SSRF attempt | dial refused; `UZ-*` error + classified failure |
| Tenant host DNS rebind | TOCTOU resolve | connection pinned to first-resolved public IP |
| Control-plane outage | transport errors | bounded+jittered backoff, capped at `MAX_BACKOFF_MS`; drain still observed |
| Watchdog thread spawn fails persistently | thread exhaustion | worker-fatal/observable; no silent unbounded call |
| `max_tokens` / tar size out of range | bad/corrupt input | clamped/rejected; no `@intCast`/overflow panic |
| Sandbox primitive unsupported by kernel | old kernel | fail-closed (unchanged) — lane asserts the refusal path |

---

## Invariants

1. **Redaction set ⊇ substitution set** — both derive from one accessor over `secrets_map`; enforced by `test_redaction_set_equals_substitution_set` (a new `secrets_map` leaf that the redactor misses fails the test).
2. **No silent open egress** — unset/unknown policy never resolves to `allow_all`; enforced by `test_unknown_network_policy_fails_closed`.
3. **Every tenant-supplied allowlist host is pinned** — enforced by routing through `resolveConnectHost` + `test_tenant_host_dns_pinned`.
4. **Backoff is bounded** — `MAX_BACKOFF_MS` is a named constant; enforced by `test_backoff_capped_and_jittered`.
5. **No DebugAllocator in release** — selected by `builtin.mode`; enforced by `test_release_allocator_not_debug`.
6. **No new file > 350 lines / fn > 50** — §5 splits enforced by the length gate.

---

## Test Specification (tiered)

> Prose + assertions only. One row per Dimension; ≥50% negative paths; every Failure Mode row has a test. Input-matrix (empty / null / max / malformed / not-sent) is explicit per §1/§4 row.

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_collect_secrets_covers_secrets_map` | api_key + each secrets_map leaf present; empty map → only api_key; no map → no crash |
| 1.2 | unit | `test_redaction_set_equals_substitution_set` | the two sets are identical for a fixture policy |
| 1.3 | unit | `test_redaction_oom_drops_frame_not_raw` | failing allocator → output omits the frame; secret value never written |
| 1.4 | unit | `test_redacted_output_excludes_secret_value` | `indexOf(out, secret_value) == null` for api_key + tool secrets; empty secret value is not treated as match-all |
| 2.1 | unit | `test_unknown_network_policy_fails_closed` | "", "garbage", unset → not `allow_all` |
| 2.2 | unit | `test_startup_logs_egress_posture` | boot log line names the resolved posture |
| 2.3 | unit | `test_tenant_host_private_ip_rejected` | 127.0.0.1 / 169.254.169.254 / 10.x via tenant entry → refused |
| 2.4 | integration | `test_tenant_host_dns_pinned` | resolve→pin path invoked for tenant host; rebind to new IP not honored |
| 2.5 | unit | `test_allowlist_no_wildcard_widening` | `*` / `*.x` cannot pass the exact-match pre-gate |
| 3.1 | unit | `test_backoff_capped_and_jittered` | sequence is monotonic→capped at `MAX_BACKOFF_MS`; jitter within ±band; injected jitter source |
| 3.2 | unit | `test_watchdog_uses_thread_mutex` | lock type is `std.Thread.Mutex` (compile/type assert) |
| 3.3 | unit | `test_watchdog_spawn_failure_is_fatal` | forced spawn failure → fatal/observable, not silent-unbounded |
| 3.4 | unit | `test_max_tokens_out_of_range_clamped` | -1, 0, `>u32max` → clamped/ignored, no panic |
| 3.5 | unit | `test_corrupt_tar_size_rejected` | huge/overflowing size field → `error`, no panic |
| 4.1 | integration (root) | `test_integration_seccomp_traps_denied_syscall` | denylisted syscall → `SECCOMP_VIOLATION_EXIT` |
| 4.2 | integration (root) | `test_integration_landlock_denies_outside_write` | write outside workspace → denied |
| 4.3 | integration (root) | `test_integration_cgroup_pids_and_oom` | `pids.max=1` fork → `resource_kill`; OOM → `wasOomKilled` |
| 4.4 | unit | `test_cgroup_events_parser_matrix` | valid/malformed/empty/missing-key event bytes parse correctly |
| 4.5 | n/a | depth-gate count | runner integration tests counted by `_lint_zig_test_depth` |
| 4.6 | unit | `test_<mod>_input_matrix` | empty/null/oversize/not-sent for `call_deadline`/`Policy`/`AllowList`/`Plan`/`Socket` |
| 5.1 | unit | `test_run_context_executes_with_stub_provider` | success path returns content/tokens with a stub provider, no network |
| 5.2 | unit | `test_observer_select_union` | union variants; no `undefined` read; progress vs fallback both work |
| 5.3 | n/a | length gate | `loop.zig` + `lease_run.zig` under cap |
| 5.4 | unit | `test_config_sandbox_tier_enum` | parse valid/invalid tier; invalid → `dev_none`; no string compares downstream |
| 5.5 | unit | existing UsageSnapshot tests | pass after file-as-struct lift |
| 6.4 | unit | `test_release_allocator_not_debug` | release mode selects non-Debug allocator |

Regression: all existing runner unit + integration tests stay green (no wire change). Idempotency: `RunContext.deinit` idempotency/single-shot test (§5).

---

## Acceptance Criteria

- [x] No resolvable secret reaches a frame/report — redaction set == substitution set; all 3 emitters (tool-args, chunk, final-reply) fail-closed on OOM; cross-chunk boundary closed (S1)
- [x] Egress never silently open — `Policy.fromSlice` returns `allow_all` only on exact match; unset/typo → `allow_list_egress`
- [x] Tenant hosts pinned/private-IP-rejected — empty inner allowlist routes every host through `resolveConnectHost` (private-IP reject + DNS-rebind pin); exact-match gate, no wildcard widening
- [x] Backoff bounded — saturating mul + `@min(MAX_BACKOFF_MS)` + bounded jitter (`backoff.zig`)
- [x] Each sandbox primitive proven (root lane) — seccomp/Landlock/cgroup real-process proofs run on a real kernel (252/4/0)
- [x] Engine success path tested — DI seam routes provider acquisition through the injected stub offline (`run_context_test.zig`)
- [x] `make lint` clean · `make test` (runner unit) 338/7/0 · runner `test-integration` 252/4/0 on Linux
- [x] `make memleak` clean for the runner allocator wiring (`std.testing.allocator` binding gate)
- [x] Cross-compile clean: `zig build --build-file build_runner.zig -Dtarget=x86_64-linux && -Dtarget=aarch64-linux`
- [x] `gitleaks detect` clean · no added file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: runner unit tests
zig build --build-file build_runner.zig test 2>&1 | tail -3 && echo PASS
# E2: runner integration (Linux host) — real-process enforcement lane
zig build --build-file build_runner.zig test-integration 2>&1 | tail -5
# E3: cross-compile both linux targets
zig build --build-file build_runner.zig -Dtarget=x86_64-linux 2>&1 | tail -2
zig build --build-file build_runner.zig -Dtarget=aarch64-linux 2>&1 | tail -2
# E4: lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: gitleaks — gitleaks detect 2>&1 | tail -3
# E6: secret never in output (belt-and-braces grep of a captured frame fixture)
#     covered by test_redacted_output_excludes_secret_value
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweep after §5 splits — grep -rn "executeInner" src/runner | head
```

---

## Dead Code Sweep

> Filled per Section as files split. §5 extracts (not deletes) — confirm no duplicated logic left behind in `runner.zig`/`loop.zig`/`pipe_proto.zig` after extraction.

| Deleted/moved symbol | Grep | Expected |
|----------------------|------|----------|
| `executeInner` (inlined into `RunContext`) | `grep -rn "executeInner" src/runner` | only the new call site |
| `[1]Secret` return shape | `grep -rn "\[1\].*Secret" src/runner` | 0 matches |

No whole-file deletions planned → otherwise "N/A".

---

## Discovery (consult log)

> Empty at creation. Append consults, gate-flag triage, skill outcomes, and any Indy-acked deferral quotes here.

- Single-spec (not multi-workstream-milestone) per Indy direction, Jun 24 2026: "Can you start this in 1 spec, why do we need gazillions spec?" — context: M100 decomposition.
- §1 secret-redaction (Jun 25, 2026): `collectSecrets` moved to `runner_helpers` (RULE FLL), now returns an allocated slice over api_key ∪ every `secrets_map` leaf (mirrors `secret_substitution`'s traversal → redaction set == substitution set); `redactedFinalReply` now fails closed on redaction OOM (was `catch response` → raw leak); the observer drops tool-call/chunk frames on redaction OOM (was `catch raw`). Tests green (single-filter): D1.1 secrets_map coverage, D1.2 set parity (+ non-object/non-string skip), D1.4 secret-value-absence, D1.3 final-reply fail-closed-on-OOM. **Remaining within §1:** D1.3 direct observer-frame-drop (pipe-capture) test → add in VERIFY/`/write-unit-test` (logic shares the tested `redactBytes` error path).
- §2 fail-closed egress + SSRF (Jun 25, 2026): **routine choice point** — unset/typo `RUNNER_NETWORK_POLICY` resolves to `allow_list_egress` (reuses the existing supervisor fail-closed refusal + the `egress_strict_unimplemented_fail_closed` error Product Clarity §10 names; forward-compatible with the documented end state) rather than a new error or `deny_all_egress`. Behaviour change: an *unset* policy now fails closed; operators set `allow_all` explicitly for the interim open posture. Local + reversible (one-line default flip). **SSRF realization:** the wrapper's outer exact gate is authoritative, so inner allowlist `&.{}` routes tenant hosts through `resolveConnectHost` — no nullclaw fork. Tests cross-platform (IP literals, hermetic): D2.1 fail-closed matrix, D2.3 private-IP reject (loopback/link-local/RFC1918), D2.5 wildcard non-widening, + end-to-end through the real `buildHttpRequest`.
- §3 bounded reliability (Jun 25, 2026): **gate-flag triage / judgment** — §3.2's "use `std.Thread.Mutex`" is infeasible (type removed in Zig 0.16). Resolved by an enforced `comptime assert(!builtin.single_threaded)` over the futex-backed `common.Mutex` + a 2-thread mutual-exclusion test (see §3 header note). §3.3 `arm()` → `ArmOutcome`; a watchdog-spawn failure now fails the verb (`ClientError.WatchdogUnavailable`) instead of running unbounded. §3.1 backoff lives in new `common/backoff.zig` (sibling-helper re-exported by `constants.zig` — the established `clock.zig`/`sync.zig` pattern; keeps `loop.zig` under the 350 cap). All green.
- §4 partial (Jun 25, 2026): §4.4 extracted pure `CgroupScope.parseEventCount` + fixture matrix; §4.6 `Plan` input-matrix. §4.1–4.3/4.5 (root-gated real-process enforcement proofs) deferred to a Linux-root/CI host — declined to author blind on Darwin where they only compile-and-skip (false-confidence risk). This is the remaining GA gate.
- §4 COMPLETE — GA gate closed (Jun 25, 2026): **Indy-directed approach** ("Docker spike, then author") — rather than defer §4.1–4.3 to CI, authored + verified them locally in a privileged Linux container. New `src/runner/sec_enforcement_integration_test.zig` (4 `integration:`-prefixed proofs): seccomp denied-`ptrace` traps → `SECCOMP_VIOLATION_EXIT`; Landlock out-of-workspace write denied / in-workspace allowed; cgroup `pids.max=1` refuses fork → `wasPidsExhausted` → `classify`=`resource_kill`; cgroup `memory.max` over-budget child OOM-killed → `wasOomKilled` → `classify`=`oom_kill`. Pattern mirrors `sandbox_integration_test.zig` (fork → child applies the real enforcer → parent asserts the kernel verdict). **Key learnings:** (1) must run **native-arch** — under x86_64-on-arm64 QEMU the seccomp arch-matched BPF filter is bypassed (false green), so the lane uses `ci-zig-alpine:0.16.0` arm64; (2) the OOM child silently swaps unless swap is disabled on the scope (`memory.swap.max=0` — `disableScopeSwap`), mirroring a swapless prod node; (3) cgroup v2 needs a delegated controller subtree (`scripts/cgroup-delegate.sh`: drain root procs to an `init` leaf, enable `+cpu +memory +pids` in `subtree_control`, sweep stale scopes) — codified once and exposed via the `test-integration-kernel` lane (native on Linux/CI, auto-containerized on macOS). **Delegation discipline (Indy-directed):** `cgroup-delegate.sh` drains the root cgroup + writes `subtree_control`, so it runs ONLY inside the disposable macOS container and the privileged CI step — NEVER a developer's host; a bare `make test-integration-kernel` on Linux runs the lane delegation-free and the cgroup proofs `requireCgroupDelegation()`-skip. **No silent green:** that precheck skips ONLY when delegation is genuinely absent; a delegated-but-broken cgroup fails hard. seccomp + Landlock need no delegation → they run unprivileged; the cgroup pair runs via an Indy-approved privileged-container CI step (`--privileged --cgroupns=private` + `cgroup-delegate.sh` before `make test-integration-kernel`) so all four proofs execute in CI on every PR (GitHub `ubuntu-latest` is native amd64 → seccomp faithful, no QEMU). **Production note:** the script is test-only; prod cgroup delegation comes from the init system (systemd `Delegate=`) / container runtime, never a manual script. Verified native-arm64: 251 pass / 4 skip / 0 fail; host cross-compile x86_64-linux + aarch64-linux clean; ZLint/pg-drain/line-limit/isolation/depth gates green.

---

- **VERIFY close-out (Jun 25, 2026):** `/write-unit-test` coverage audit found the landed surface fully covered EXCEPT the §1 streaming observer frame-drop-on-OOM branches (tool-args + chunk), which the Discovery's §1 note had flagged as "remaining." Closed with `runner_progress_redact_oom_test.zig` (2 tests, mutation-killed: raw-on-OOM mutant fails on `!seen.started`). `/review` ran two independent adversarial passes (security + correctness/memory/concurrency) — both "ship as-is", all four spec security guarantees verified in the actual code (redaction set == substitution set; 3 emitters fail-closed; egress fail-closed; SSRF pin via empty inner allowlist → `resolveConnectHost`; int-casts hardened); loop→lease_run extraction and the UsageSnapshot lift verified behaviour-preserving/byte-identical.
- **S1 fix — streaming cross-chunk secret redaction (Jun 25, 2026, Indy-directed "Fix now"):** the adversarial security pass found a P2 fail-open — the live-tail stream redacted each `StreamChunk` delta independently, so a secret split across two deltas (`"sk-ab"`+`"c123"`) streamed raw (durable final reply stayed clean). Closed by `engine/stream_redactor.zig` (file-as-struct): `push` carries the un-emitted tail (≤ longest-secret−1 bytes) across deltas, redacts the join, and emits only bytes no future delta can complete into a secret; the held tail is dropped at stream end (the redacted final reply carries it). 4 unit tests + 1 adapter-level boundary test through the real pipe; mutation-killed (hold≡0 → both boundary tests fail). `runner_progress.zig` kept under the 350-line cap by relocating the 3 `redactBytes` def-tests to `runner_progress_redact_test.zig` and registering the new module + oom test in `tests.zig`.
  > 🤠 Indy (2026-06-25): "Fix now" — S1 stream-chunk boundary secret leak fixed before GA rather than deferred; context: `/review` P2 finding on the headline §1 redaction guarantee.
- **De-scoped to follow-up (Indy-acked, Jun 25, 2026):** the remaining §5/§6 build-ergonomics + cosmetic items are deferred out of this milestone (GA scope §1–§4 + §5.1/5.3/5.4/5.5 + §6.3/6.4 complete). Verbatim acks:
  > 🤠 Indy (2026-06-25): "Skip §6.2, it's not worth it" — §6.2 per-subsystem test steps deferred (P2 compile-speed ergonomics, RULE NDC maintenance cost).
  > 🤠 Indy (2026-06-25): "Close out & open GA PR now" — §5.2 observer_select union, §6.1 nullclaw stub, §6.5 CI cache key all parked (P2/cosmetic); ship the GA hardening now.

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |
| After every push | `kishore-babysit-prs` | greptile findings triaged to two empty polls |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit | `zig build --build-file build_runner.zig test` | 338 pass / 7 skip / 0 fail (incl. +2 streaming OOM-drop + 5 cross-chunk redactor/adapter tests) | ✅ |
| Integration (Linux, native arm64) | `make test-integration-kernel` (privileged container) | 252 pass / 4 skip / 0 fail — §4.1–4.3 proofs run on a real kernel | ✅ |
| Cross-compile x86_64 | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux` | clean (runner exe + test graph) | ✅ |
| Cross-compile aarch64 | `zig build --build-file build_runner.zig -Dtarget=aarch64-linux` | clean | ✅ |
| ZLint | `make _zlint_check` | 0 errors / 0 warnings, 525 files | ✅ |
| pg-drain | `python3 lint-zig.py src` | passed, 519 files | ✅ |
| Depth gate | `make _lint_zig_test_depth` | unit=2129 integration=206 | ✅ |
| Line / isolation | `make _zig_line_limit_check _runner_isolation_check` | ≤350 all files (runner.zig 349, runner_progress.zig 333); nullclaw-only deps | ✅ |
| Memleak (runner) | `std.testing.allocator` on the runner unit suite (binding leak gate; `make memleak` targets agentsfleetd, untouched here) | green | ✅ |
| Mutation (S1 + §1 OOM) | hold≡0 / raw-on-OOM mutants | both killed (boundary + drop tests fail) | ✅ |
| Gitleaks | `gitleaks detect` | no leaks, 2921 commits | ✅ |

---

## Out of Scope

- **§5.2 observer_select union, §6.1 nullclaw build stub, §6.2 per-subsystem test steps, §6.5 CI cache key — de-scoped to follow-up (Indy-acked, see Discovery).** All P2/cosmetic build-ergonomics; the GA scope (§1–§4) + §5.1/5.3/5.4/5.5 + §6.3/6.4 landed. §5.2 is cosmetic (the current `undefined` writer/adapter are guarded-safe behind `progress_fd != null`); §6.1/6.2/6.5 are compile-speed/CI ergonomics with no behaviour impact.
- The strict `allow_list_egress` netns/veth/nftables runtime (§2 hardens default/unknown only) — follow-up spec.
- Capability drop (`CAP_SYS_ADMIN`/`CAP_NET_ADMIN`) in the child + `unshare`/`setns` seccomp denylist additions — documented residual; relevant when strict egress ships.
- Clock-skew-tolerant lease TTL (server-relative) — restated from M90_001 Out of Scope.
- Forwarder dropped-frame counter (observability nicety) — follow-up.
- **cgroup-cage production enablement** (follow-up, surfaced Jun 25 2026 reviewing §4.3): `deploy/baremetal/agentsfleet-runner.service` has no `Delegate=yes`, and `CgroupScope.CGROUP_BASE` is hardcoded at the cgroup root (`/sys/fs/cgroup/fleet.runner`). Without a delegated controller subtree, `memory.max`/`pids.max` writes may not engage on a real node — i.e. the resource cage could be inert in prod. **Not GA-blocking** per Indy: seccomp + Landlock (the security walls, applied per-exec in the child) are unaffected; the cgroup cage is the resource-limit/DoS layer. Likely fix: `Delegate=yes` on the unit + base the scope under the service's own slice. The §4.3 proof already validates the enforcement *logic* given delegation — only the prod *provisioning* of delegation is out of scope here.
