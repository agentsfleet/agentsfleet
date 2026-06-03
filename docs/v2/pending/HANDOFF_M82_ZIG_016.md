# HANDOFF — M82_001 Zig 0.15.2 → 0.16.0 toolchain migration

> Ephemeral pickup brief. Delete at CHORE(close); do NOT ship in the PR.
> Authored Jun 03, 2026 from session that proved feasibility + wrote the spec.

## What this is

Migrate the entire build off Zig 0.15.2 onto 0.16.0 — deps, our source, CI.
Feasibility is **proven** (a spike compiled the full dependency graph on 0.16).
The spec is written and committed; your job is to execute it.

- **This branch (`feat/m82-zig-016-toolchain`)** already carries the docs: this handoff + the spec (`docs/v2/pending/M82_001_P2_API_INFRA_ZIG_016_TOOLCHAIN_MIGRATION.md`, PENDING) + its Discovery log. Read the spec fully first — Sections, Dimensions, tests, Failure Modes, Invariants. `main` is clean (no M82 commits).
- **Proven foundation:** in `stash@{0}` on this machine (message "M82 zig-0.16 foundation…") — the dep re-pins + both vendored forks + `versions.env`/`.gitignore`. It is NOT committed (it can't pass hooks until the migration lands — see warning below). Restore with `git stash pop`. **If the stash is gone** (different machine / cleared), reproduce it from "Foundation contents" at the bottom of this doc — every pin/hash/SHA is recorded there.

## Start here (CHORE(open))

```bash
git checkout feat/m82-zig-016-toolchain            # this branch already has spec + handoff
git worktree add ../usezombie-m82-zig-016 feat/m82-zig-016-toolchain && \
  cd ../usezombie-m82-zig-016 && bun install && (cd zombiectl && bun install && bun run build)
# CHORE(open): move spec pending/ -> active/, set Status: IN_PROGRESS + Branch:, commit (no code yet)
# then restore the proven foundation:
git stash pop   # build.zig.zon re-pins, vendor/httpz (zig-0.16 + UAF patch), vendor/zig-yaml fork, versions.env 0.16.0, .gitignore zig-pkg/
# add repo-local .mise.toml pinning zig=0.16.0 so the hooks build with 0.16 from here on
```

> ⚠️ The migration **cannot land partially**. Pre-commit hooks build deps with the
> mise-pinned Zig. On 0.15.2 the new 0.16 deps fail to compile; on 0.16 our
> un-migrated src fails. So the mise pin + deps + src migration must flip in ONE
> atomic B1 landing. Never `--no-verify`.

## ⛳ Prerequisites — both now RESOLVED (Jun 03, 2026)

1. **zlint on 0.16 — ✅ VERIFIED COMPATIBLE, not a blocker.** zlint `v0.8.1`'s parser was run via `--print-ast` against **all 575** real Zig 0.16 source files (the entire 0.16 std library recursively + the vendored 0.16 deps) — **zero parse errors** (negative control confirmed the test catches real failures). 0.16 was a std-library reform, not a grammar change, so the pinned `ZLINT_VERSION: v0.8.1` in `lint.yml` stays as-is. No fork, no deferral needed. Re-confirm once your own migrated `src/` exists (cheap: `make lint`), but the grammar question is settled.

2. **ci-zig 0.16.0 images — ✅ BUILT + PUSHED (Jun 03, 2026).** `versions.env` is bumped to 0.16.0 (authoritative ziglang.org SHAs) and all three `ghcr.io/usezombie/ci-zig-{alpine,debian-trixie,ubuntu}:0.16.0` images are in GHCR. Your job is only to **flip the workflow tags** (`:0.15.2`→`:0.16.0`) — no baking required. Verify the tags exist: `gh api /orgs/usezombie/packages/container/ci-zig-ubuntu/versions --jq '.[].metadata.container.tags[]'`. If you need to rebuild: `cd playbooks/013_ci_zig_images && ./build_and_push.sh fetch-shas 0.16.0 && ./build_and_push.sh build` (needs docker buildx + `gh` with `write:packages`).

## Local toolchain switch (mise) — already mostly handled

- Zig 0.16.0 is **already installed** via mise (`mise ls zig` shows it).
- Only the global pin (`~/.config/mise/config.toml`) says 0.15.2. Add a **repo-local `.mise.toml`** pinning `zig = "0.16.0"` as part of the diff — it scopes the switch to this repo and is the in-repo source of truth the CI/version-consistency invariant checks against.

## The work, in order (Batches from the spec)

- **§1 deps** — `git stash pop` already did most of it. Verify `zig build --fetch` + `zig build` green on 0.16.
- **§2 wall-clock keystone** — new `src/lib/common/clock.zig` (`nowMillis`/`nowNanos`), then mechanically redirect ~206 sites (`git grep -l 'std\.time\.\(milli\|nano\)Timestamp' src/`). Orphan sweep must hit 0 (RULE ORP).
- **§3 std renames** — `GeneralPurposeAllocator`→`DebugAllocator` (3), `fixedBufferStream`→`Io.Writer.fixed` (10), HashMap unmanaged (~6), ArrayList 0.16 API.
- **§4 CI cutover** — bake images (above), flip 9 workflow files `:0.15.2`→`:0.16.0` + 3 `mlugg/setup-zig` `version:` lines, `versions.env`, repo `.mise.toml`.
- **§5 rule reconciliation** — RULE ZAL is 0.15-pinned; update it to 0.16 in `docs/greptile-learnings/RULES.md`. Upstream the zig-yaml fix to kubkon/zig-yaml in parallel (external, non-gating).
- **OUT OF SCOPE → M82_002:** the `std.http.Client` rewrite (5 sites, auth boundary) — its own spec+PR.

## Toolchain prerequisite summary

| Item | State | Action |
|------|-------|--------|
| `ci-zig-*:0.16.0` images | ✅ built + pushed to GHCR (Jun 03) | flip workflow tags only |
| zlint 0.16 | ✅ v0.8.1 parses all 575 0.16 files clean | none — keep `ZLINT_VERSION: v0.8.1` |
| mise local Zig 0.16.0 | ✅ installed; global pin still 0.15.2 | add repo-local `.mise.toml` zig=0.16.0 |
| `mlugg/setup-zig` lanes | ✅ 0.16.0 available upstream | flip `version:` in 3 workflows |
| `versions.env` SHAs | ✅ bumped to 0.16.0 (in foundation stash) | none |
| bun/node/python/actionlint/openssl/gitleaks | ✅ unaffected (Zig-only migration) | none |

## Verify (the gate before PR)

`make test` · `make test-integration` · `make memleak` (UAF regression guard for the re-vendored httpz) · `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` (RULE XCC) · orphan sweeps for `std.time.*Timestamp` and `0.15.2` in CI = 0 · `make lint` (this is where zlint runs — see prerequisite #1).

## Pitfalls banked from the spike

- httpz UAF patch (`vendor/httpz/src/worker.zig`, `stop()` before `deinit()` in non-blocking `Worker.deinit`) is **verified still required** on upstream's zig-0.16 branch. `make memleak` on the Linux non-blocking loop is the regression guard. Don't drop it.
- zig-yaml: library is 0.16-clean; its conformance harness (`test/spec.zig`) uses removed std APIs — the vendored `build.zig` drops that test wiring. Don't re-enable it.
- `zig-pkg/` (27M dep cache) is now gitignored — don't commit it.
- posthog v0.2.0's `init` gained an `io` param — the one call site is `src/zombied/cmd/preflight.zig`.

## Foundation contents (reproduce if `stash@{0}` is gone)

Everything below is in the stash; recorded here so the branch is self-sufficient. Apply by editing `build.zig.zon` to these pins, re-vendoring the two forks, and bumping `versions.env`.

**`build.zig.zon` — `minimum_zig_version = "0.16.0"` and these dependency pins:**

```zig
.nullclaw = .{
    .url = "git+https://github.com/nullclaw/nullclaw.git?ref=v2026.5.29#b25c6eb59f845f4f8cdbb50d6f284cb56d723435",
    .hash = "nullclaw-2026.5.29-wlZZyaGGRwHMvCfJnfcJBv1If_R-YN6xHG54_y-Kk4l1",
},
.pg = .{
    .url = "git+https://github.com/karlseguin/pg.zig?ref=master#1aa3e3c790b6f7fe7ad76052728db3198069d3eb",
    .hash = "pg-0.0.0-Wp_7gWrzBgBmZY2OJt5g-x7dm69orG2D0u5gYMmboir-",
},
.posthog = .{
    .url = "git+https://github.com/usezombie/posthog-zig.git#v0.2.0",
    .hash = "posthog-0.2.0-QwvZlsuGAgBOvEKmQ-yKf77yaLyP7SeCCPanYW1pRl7X",
},
.zbench = .{
    .url = "git+https://github.com/hendriknielaender/zBench?ref=zig-0.16.0#c24d128bfa80ec6a1b495b84ec827c6a0bec7865",
    .hash = "zbench-0.11.2-YTdc76Q_AQAxonIKZ2-H1PdcESJGwyuYylw6RkPiBqyx",
},
.httpz = .{ .path = "vendor/httpz" },
.zig_yaml = .{ .path = "vendor/zig-yaml" },
```

**Re-vendor `vendor/httpz`** — verbatim copy of `karlseguin/http.zig` branch `zig-0.16` @ `40be022616e50aa315ec9231bbc7a136ff3c1f33`, then re-apply the UAF patch: in non-blocking `Worker.deinit` (`src/worker.zig`), add `self.thread_pool.stop();` immediately before `self.thread_pool.deinit();`. Update `vendor/httpz/CHANGES.md` accordingly. (Websocket stays commented out upstream — we don't use it.)

**Re-vendor `vendor/zig-yaml`** — verbatim copy of `kubkon/zig-yaml` branch `main` @ `84d747bc80937a08ea1cf76a63fee12c5fb1dd61` (0.3.0-dev), then in its `build.zig` remove the top-level `const SpecTest = @import("test/spec.zig");` and the `enable_spec_tests` block (the conformance harness uses removed std APIs and poisons consumers; library source itself is 0.16-clean). Add a `vendor/zig-yaml/CHANGES.md` documenting it.

**`playbooks/013_ci_zig_images/versions.env`** — `ZIG_VERSION=0.16.0` with ziglang.org SHAs:
- `ZIG_SHA256_X86_64_LINUX=70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00`
- `ZIG_SHA256_AARCH64_LINUX=ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17`

(Regenerate with `./build_and_push.sh fetch-shas 0.16.0`.) **`.gitignore`** — add `zig-pkg/`.

