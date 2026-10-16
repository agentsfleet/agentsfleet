# Codex review prompt: the `mod.rs` module-layout move

Paste the block below into Codex. It reviews the whole layout move, read-only,
and ends with one verdict line.

```text
Review the Rust module-layout move in agentsfleet fully, for correctness and completeness.
Read-only: do not edit, stage, commit, push or run destructive git. Report findings only.

Where:
- Worktree `/Users/kishore/.codex/worktrees/fd5a/agentsfleet`, branch `feat/website-completion`.
- The move is unstaged working-tree change. Other M227 edits sit in the same tree.
- Judge paths against the working tree, not the index. `make check-architecture-doc` and
  `make lint-rustd` read `git ls-files`, so they FAIL on unstaged new paths until staging.
- The read-only git shim `/tmp/modmove/shim/git` gives the post-staging view; put it first on PATH.

The rule: every Rust module that has children keeps them in its own folder.
- A module root is `x/mod.rs`.
- An integration-test program is `tests/x/main.rs`.
- No `x.rs` sits beside an `x/` folder.
- Every crate root carries `#![deny(clippy::self_named_module_files)]`.
- Prior art: `2b4e20d28` (the runner crates). Read its message and a few diffs first.

What the move claims (verify every claim; do not trust it):
1. First pass: 240 files moved, 236 under `src/` and 4 nested test modules.
   - 86 `#[path]` attributes deleted and 6 re-based.
   - The deny lint was added to 43 crate roots.
   - Four include paths were fixed: three `.lua` scripts in `afd_dragonfly/src/session/mod.rs` and
     `docs/metrics.census.tsv` in `afd_observability/src/metrics/registry/mod.rs`.
2. Second pass: 47 files moved.
   - 10 source files went to `x/mod.rs`: `afd_core/src/error_code`, `afd_dragonfly/src/hub`,
     `afd_db/src/config`, `afd_api_operator/src/handler/admin/platform_keys`,
     `agentsfleetd/src/telemetry`, `afd_api_tenant/src/handler/{stream,tenant/models}`, and
     `afr_sandbox/src/toolbox/{adopt,loop_device,manifest}`.
   - 10 auto-discovered test programs went to `tests/x/main.rs`.
   - 27 suite-mounted test modules went to `tests/x/mod.rs`.
   - The full map is `/tmp/modmove2/moves.tsv`.
   - 158 `#[path]` were deleted, 14 re-based by `../`, and 1 added
     (`afr_providers/tests/providers/main.rs:15`, `#[path = "../support/mod.rs"] mod support;`).
     The list is `/tmp/modmove2/path_edits.txt`.
   - Two `include_str!` were re-based (`afd_api/tests/webhook_fleet_route/mod.rs:40,44`).
   - The before/after file diff is `/tmp/modmove2/audit.diff`.
   - The `serve` module was moved separately by the length-cap work: `agentsfleetd/src/serve/mod.rs`
     plus `serve/listen.rs`.
3. Targets and tests are unchanged.
   - `cargo metadata` lists 150 (package, target, kind) rows before and after; only the 10
     test-program `src_path`s changed.
   - `cargo test --workspace --all-features -- --list` has 5510 `: test` lines before and after,
     with identical names.
   - Evidence: `/tmp/modmove2/{metadata,targets,testlist,perbin}_{before,after}.*`.
4. A re-scan finds no `x.rs` beside an `x/` folder under `rustd/crates`, except `lib.rs`, `main.rs`
   and `mod.rs`.
5. References:
   - Live citations were rewritten across `docs/` (history in `docs/v*/done/**` and
     `docs/reviews/**` is left as written), `deploy/fly` comments, Rust and Cargo comments,
     `schema/400_model_library.sql`, and `ui/packages/app/lib/api/model_library{,.test}.ts`.
   - `.orly/docs` paths are fixed upstream in orly PR #61 and arrive with orly 0.14.2.
   - `.github/actions/build-toolbox/action.yml:7,178` now name `toolbox/manifest/mod.rs` (the owner
     approved it). Line 188 `awk`s the release public key out of that file; confirm it still extracts
     the full key.
   - `cli/test/acceptance/fixtures/command-matrix.ts:258` cites `tenant/models.rs`; that is in your
     lane.
6. Coverage tooling:
   - `RUSTD_COVERAGE_IGNORE` is now `(/src/test_util(/mod)?\.rs$|/crates/afd_bench/)`.
   - The four toolbox kernel-lane files are graded again (the owner's call). `codecov.yml` matches.
   - `scripts/rustd_coverage.py --per-file-floor` (`RUSTD_PER_FILE_FLOOR ?= 99`) grades every file
     a patch touches, whole.

Check:
1. No behaviour change. `git diff -M --stat` shows renames; any content change beyond deleted or
   re-based `#[path]`, re-based include strings and comment paths is a finding.
2. Module resolution. Every `mod` still resolves to the same file it did before. A crate-root file
   resolves `mod child;` beside itself, and a `#[path]`-loaded file acts as a `mod.rs` for its
   children. Look for any grandchild that silently changed file.
   `cargo build --workspace --all-targets --all-features` must succeed.
3. Targets and tests. Recompute both lists yourself, building HEAD in a scratch `git worktree` if
   you need the before view. Ignored (live-datastore) tests must still be listed.
4. The `tests/x/mod.rs` versus `tests/x/main.rs` choice. Is each of the 27 truly mounted by a suite
   and not a target? Would `autotests = true` ever turn one into a duplicate target?
5. File-relative paths: `include_str!`, `include_bytes!`, `include!`, `file!()`, `CARGO_MANIFEST_DIR`
   joins, and fixture reads relative to a moved file.
6. Path-naming tooling: `RUSTD_COVERAGE_IGNORE`, `codecov.yml`, `.github/workflows/**` and
   `.github/actions/**`, `scripts/`, `make/`, and any `paths:` filter. A filter that no longer
   matches is a silent gap.
7. Stale citations: search for every old `x.rs` path, and bare names such as `hub.rs`, across live
   docs, scripts, CLI, UI, playbooks and comments.
8. Lint: `cargo clippy --keep-going --workspace --all-targets --all-features -- -D warnings` must
   give 0 findings. Prove the deny lint fires: a scratch `foo.rs` beside `foo/` must fail clippy.
   Do that only in a scratch copy, never in the worktree.
9. Diagnostics: `agentsfleetd/src/fatal.rs` (`ours_only`) cuts backtrace frames to the last path
   segment, so a panic in a moved file prints `mod.rs:LINE:COL`. Does that lose information an
   operator needs? Recommend the smallest fix only.
10. Prior-art fit: compare against `2b4e20d28` and the arrow-rs layout (`~/Projects/oss/rs/arrow-rs`).
    Cite Microsoft Pragmatic Rust Guidelines IDs (`~/Projects/oss/rust-guidelines/all.txt`, read by
    section) where relevant, for example `M-BALANCED-MODULES`.

You may run read-only commands, cargo build, test and clippy, and `make test-unit-rustd` (run it
alone; two runs in one tree race on a shared tally file). Do NOT run `make test-integration-rustd`.

Output:
1. Findings ranked P0 to P3, each with `file:line`, what is wrong, a concrete failure, and the fix.
2. Commands run with their decisive output lines, including target and test counts before
   versus after.
3. The last line is exactly one of: `MOVE SOUND` or `MOVE NOT SOUND: <the P0 and P1 items>`.
```
