<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M213_001: Cutover — the release builds and deploys the Rust runner with its signed toolbox, the Zig runner with its lanes and images is deleted, and nothing in the repository or the published docs describes Zig or NullClaw as current

**Prototype:** v2.0.0
**Milestone:** M213
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — until this lands, two runners exist and only one is deployed; every fleet still runs on the Zig runner
**Categories:** API, DOCS, INFRA
**Batch:** B1 — one Pull Request; the dev lane deploys the Rust runner from the branch before merge, so the proof precedes the deletion
**Branch:** feat/m213-rust-runner-cutover
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_002 · M211_001 · M211_002 · M212_001 · M214_001 — everything the Zig runner serves today runs on Rust first; the Rust runner exports its own spans and metrics (M214_001, Indy: "Own spec, before cutover"); M211_001's spikes S2–S4 are recorded, because their results fix the artifact layout this release freezes; M213_002 (the sandbox egress allowlist) ships in this Pull Request, because on `allow_all` today's Zig child shares the host network and the Rust sandbox reaches nothing until it lands (Indy, Oct 06, 2026: "create a new spec and make it depend on M213 and they must ship in 1 PR")
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from an inventory on `main`: `src/`, `build.zig`, `build.zig.zon`, `build_runner.zig`, `make/*.mk`, `.github/workflows/*.yml`, 510 Rust files mentioning Zig or NullClaw, 12 architecture pages
**Canonical architecture:** `docs/architecture/runner_execution.md` (the runner this deploys); `docs/architecture/runner_fleet.md` §"The split — two binaries, no sidecar"; `docs/architecture/testing.md` §"Public lanes"

---

## Overview

**Goal (testable):** `test_no_zig_sources_remain` — after the cutover, `git ls-files '*.zig'` is empty, `build.zig`, `build.zig.zon` and `build_runner.zig` are gone, `make test-unit-all` and `make lint-all` pass on a machine with no `zig` on `PATH`, `make check-version` passes reading only `rustd/Cargo.toml` and `cli/package.json`, `grep -rci 'zig\|nullclaw' rustd/crates --include='*.rs'` sums to 0, the release workflow ships the static `agentsfleet-runner-linux-amd64`, the content-addressed toolbox image with its signed manifest and Software Bill of Materials (SBOM), and an offline bundle of both, and the dev lane's deployed runner registers with a capability report and runs the four reference bundles.
**Problem:** The Zig runner is what `release.yml` builds (`compile-runner-amd64` on `ghcr.io/agentsfleet/ci-zig-alpine`), what `deploy-dev-build.yml` ships, what `deploy-dev-metal.yml` installs (`RUNNER_BINARY`), what `make test-unit-runner` and `lint-runner-fmt` test, and what `check-version` reads (`build.zig.zon`). 510 Rust files carry 1,684 lines naming Zig or NullClaw as the thing they mirror, and 12 architecture pages describe a NullClaw child as the workload. Indy's rule for the Rust runner is "none of the rust code will point to the zig", and the published tools page lists the Zig runner's tools.
**Solution summary:** The release and dev workflows build the Rust binary static in the daemon's Alpine job shape, and build the toolbox on a Debian builder where Syft writes its SBOM, Grype scans it and cosign signs its manifest; the image is published apart from the binary, with an offline bundle of both. The metal deploy stages them, and the runner admits the toolbox by descriptor (M211_001 §8). Then the Zig tree, its build files, the NullClaw dependency, its make targets, its formatting job, its cache families and its images go. Comments and tests in `rustd` stop naming Zig or NullClaw; the three CLI comments point at Rust paths; the architecture pages describe the Rust runner as current and keep Zig only in dated Decisions and history rows. The published docs gain the full tool catalog, the install of the binary and its signed toolbox, and a changelog entry on their own branch. The previous release's Zig artifact remains the rollback.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner)!: deploy the Rust runner; retire the Zig runner and NullClaw
- **Intent (one sentence):** Every fleet runs on the Rust runner, the repository builds and tests one runner, and a reader finds nothing that says otherwise.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `.github/workflows/release.yml` — the daemon's Alpine musl job (static Executable and Linkable Format (ELF), zero `NEEDED`) is the shape the runner binary takes; `compile-runner-amd64` is what changes.
2. `.github/workflows/deploy-dev-build.yml`, `.github/workflows/deploy-dev-metal.yml` — the dev lane: build from the branch, install `RUNNER_BINARY` on metal; both gain the toolbox.
3. `make/build.mk`, `make/test-unit.mk`, `make/quality.mk`, `make/dev.mk`, `make/test.mk` — every Zig target, cache variable and version check to remove or rewrite.
4. `docs/architecture/runner_fleet.md` §"Running one event (NullClaw)", §"The split — two binaries, no sidecar"; `docs/architecture/testing.md` §"The Zig daemon is frozen and unmeasured" — the pages that describe Zig as current.
5. `docs/CHANGELOG_VOICE.md` and `dispatch/write_changelog.md` — the entry's voice; history stays.
6. `docs/architecture/runner_execution.md` §Toolbox — what the release record pins, signing and scanning, eager distribution; `scripts/toolbox/build.sh` needs root and mmdebstrap, so the toolbox builds on a Debian builder, not in the Alpine job.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/`, `src/lib/`, `src/build/`, `build.zig`, `build.zig.zon`, `build_runner.zig` | DELETE | The Zig runner, its wire-type library, its build and the NullClaw dependency |
| `.github/workflows/release.yml`, `.github/workflows/deploy-dev-build.yml`, `.github/workflows/deploy-dev-metal.yml` | EDIT | Build the binary static with cargo; build, scan and sign the toolbox on a Debian builder; publish the image, its manifest and signature, the SBOM and the offline bundle; stage both on metal. Needs Indy's explicit approval (§Hard Safety) |
| `.github/workflows/lint.yml`, `.github/workflows/bench.yml`, `.github/workflows/cache-prune.yml`, `.github/workflows/deploy-dev.yml`, `.github/workflows/deploy-dev-verify.yml` | EDIT | The `zig fmt` job, the `ci-zig-ubuntu` bench image, the Zig cache families and the Zig comments go. Same approval |
| `make/build.mk`, `make/test-unit.mk`, `make/quality.mk`, `make/dev.mk`, `make/test.mk`, `make/bench.mk`, `Makefile` | EDIT | `test-unit-runner`, `lint-runner-fmt`, `RUNNER_ZIG_VERSION`, Zig cache variables and `zig-out` cleanup go; `sync-version` and `check-version` read `rustd/Cargo.toml` and `cli/package.json` only; help text follows |
| `.oracle/orly.json` | EDIT | `surfaces.user` drops `src/` |
| `rustd/crates/**/*.rs` (510 files) | EDIT | Comments and test names stop naming Zig or NullClaw; a fact that still matters is restated about the Rust side |
| `cli/src/lib/model-catalogue.ts`, `cli/src/commands/api_key.ts`, `cli/src/commands/fleet_secret_body.ts` | EDIT | Three comments point at the Rust paths they mirror |
| `docs/architecture/runner_fleet.md`, `testing.md`, `capabilities.md`, `data_flow.md`, `memory.md`, `high_level.md`, `user_flow.md`, `README.md`, `concurrency.md`, `billing_and_provider_keys.md`, `scaling.md`, `runner_execution.md` | EDIT | The Rust runner is current; NullClaw sections are replaced by pointers to `runner_execution.md`; Decisions and history rows keep their dated facts |
| `scripts/check_architecture_doc.sh`, `scripts/check_architecture_doc_test.sh` | EDIT | A new check: no architecture page describes a NullClaw child as the workload outside a Decisions or history row |
| `VERSION`, `rustd/Cargo.toml`, `cli/package.json` | EDIT | The cutover is a release: one minor bump, synced |
| `docs/architecture/runner_execution.md` | EDIT | Decisions row: the cutover date and the rollback artifact |
| `rustd/crates/agentsfleet_runner/src/main.rs` | EDIT | The binary builds the agent loop and serves leases; `NO_AGENT_ENGINE` and its refusal leave (M210_002 review P1-7) |
| `rustd/crates/afr_sandbox/src/toolbox/manifest.rs` | EDIT | `TOOLBOX_RELEASE_PUBLIC_KEY` becomes the release key's public half, replacing M211_001's fixture key |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_telemetry.rs` | EDIT | `test_e2e_runner_lease_trace_reaches_a_collector` carries a real lease through the binary |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | `ReadWritePaths` gains the storage home, `Delegate=` gains `io`, `OTEL_SERVICE_INSTANCE_ID=%H` (§6) |
| `docs/architecture/lease_flow.md`, `docs/architecture/connectors.md`, `docs/architecture/scenarios/github-pr-reviewer.md`, every other `docs/architecture/**` page naming Zig | EDIT | The lease-flow trace reconciled claim by claim; no architecture page names Zig (§3) |
| `rustd/crates/afr_tools/src/sandbox/apply_patch.rs`, `apply_patch/size_tests.rs`, `rustd/crates/afr_tools/src/sandbox/files.rs` | EDIT / CREATE | A patch that would grow a file past the executor's read cap is refused before anything lands (§7) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC and ORP (every deleted target, variable, image and comment leaves with all its references), NLR (touch-it-fix-it on the comments), NLG (no new "legacy" framing), UFS, OBS, TST-NAM, TCF.
- `dispatch/write_shell.md` — the make and script edits: quoted expansions.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md`; `dispatch/write_changelog.md` → `docs/CHANGELOG_VOICE.md` — the published pages and the entry.
- `AGENTS.orly.md` §Hard Safety — CI/CD edits need Indy's explicit in-session approval; releases are his to cut.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit guard | yes — eight workflow files | Stop at EXECUTE for Indy's explicit approval, named per file; R4 records it |
| Architecture consult | yes | Twelve pages revised in the same Pull Request; the new check in `check_architecture_doc.sh` keeps them honest |
| LENGTH / UFS / LOGGING / MILESTONE-ID | yes | Comment edits only in Rust; no new source |
| Version sync | yes | `make check-version` after the bump |
| Schema guard | no | No schema change |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/release.yml` lines 91–148 — the daemon's Alpine musl build with the static-ELF verification; the runner and executor jobs copy it.
- **Reference:** `docs/v2/done/M187_001` and the Zig daemon's retirement recorded in `docs/architecture/testing.md` §"The Zig daemon is frozen and unmeasured" — the last time a Zig binary left this repository; the same sweep shape, one binary further.

## Sections (implementation slices)

### §1 — The release ships two artifacts and the metal deploy installs them

`compile-runner-amd64` builds `agentsfleet-runner` with cargo for `x86_64-unknown-linux-musl` in the daemon's Alpine job and verifies it static (zero `NEEDED`, no `INTERP`). `build-toolbox-amd64` runs `make toolbox-image` on a Debian builder, which writes the image and its release manifest; Syft writes the SBOM, Grype scans it with Debian findings reconciled against Debian's security tracker, and cosign signs the manifest with the release key. A fixable Critical finding the tracker does not mark not-affected fails the job; the rest ship as the scan report. The release uploads the binary, `toolbox-<sha256>.erofs`, its manifest and signature, the SBOM, the scan report, and an offline bundle of them all. `deploy-dev-build.yml` does the same from the branch. `deploy-dev-metal.yml` copies the files into the runner's toolbox staging directory and restarts the unit; the runner verifies, publishes and admits the toolbox (M211_001 §8) and reports its capabilities.

- **Dimension 1.1** — The binary is static; the toolbox is content-addressed, its manifest verifies against the release public key, and the SBOM and offline bundle ship with it → Test `test_release_artifacts_are_static_and_addressed`
- **Dimension 1.2** — The dev lane deploys the Rust runner; it registers and its capability report names `/dev/kvm` and the toolbox filesystem → Test `test_dev_runner_registers_with_capabilities`
- **Dimension 1.3** — The four reference bundles run on the dev runner from their channels and webhooks → Test `test_reference_bundles_run_on_dev`
- **Dimension 1.4** — The binary takes leases through `afr_agent::Loop` over the provider registry; `NO_AGENT_ENGINE` and its refusal are gone, which 1.2 and 1.3 rely on → Test `test_runner_binary_runs_a_lease`
- **Dimension 1.5** — `main.rs` composes `afr_supervisor::run` with the engine and its boot sweep, and M214_001's telemetry keeps working end to end: a real lease through the binary reaches the collector → Test `test_e2e_runner_lease_trace_reaches_a_collector`

### §2 — The Zig runner leaves, and the lanes do not miss it

The Zig tree, the three build files and the NullClaw dependency are deleted. `test-unit-runner`, `lint-runner-fmt`, `RUNNER_ZIG_VERSION`, the Zig cache variables and the `zig-out` cleanup go; `test-unit-all` and `lint-all` no longer depend on them; `sync-version` and `check-version` read the two remaining manifests. The `zig fmt` job, the bench image and the cache-prune families go; the `ci-zig-alpine` and `ci-zig-ubuntu` images stop being referenced (deleting them from the registry is an operator step, recorded in Session Notes). `.oracle/orly.json` stops listing `src/` as a surface.

- **Dimension 2.1** — No `.zig` file and no Zig build file is tracked → Test `test_no_zig_sources_remain`
- **Dimension 2.2** — `make test-unit-all` and `make lint-all` pass with no `zig` on `PATH` → Test `test_lanes_need_no_zig`
- **Dimension 2.3** — `make check-version` passes reading `rustd/Cargo.toml` and `cli/package.json` only → Test `test_check_version_reads_two_manifests`
- **Dimension 2.4** — No workflow, make file, script or `.oracle/orly.json` names `zig`, `nullclaw`, `zig-out` or `build_runner` → Test `test_repository_config_names_no_zig`

### §3 — Nothing describes Zig or NullClaw as current

Every Rust comment and test name that mirrors, ports or compares to Zig or NullClaw is rewritten to state the fact about the Rust side, or removed when the fact was only historical; the three CLI comments point at Rust paths. The twelve architecture pages describe the Rust runner: `runner_fleet.md` §"Running one event (NullClaw)" becomes a pointer to `runner_execution.md`, `testing.md` drops its frozen-daemon section, and the rest lose their NullClaw child. Dated Decisions and history rows keep their quotes. A new architecture check refuses a page that describes a NullClaw child as the workload outside such a row.

- **Dimension 3.1** — `rustd/crates` holds no line naming Zig or NullClaw → Test `test_rust_tree_names_no_zig`
- **Dimension 3.2** — The architecture check passes, and fails on a fixture page that reintroduces the NullClaw child → Test `test_architecture_describes_the_rust_runner`
- **Dimension 3.3** — The CLI's three comments name the Rust files they mirror → Test `test_cli_comments_point_at_rust`
- **Dimension 3.4** — `lease_flow.md` is the end-to-end walk: every claim either links to the page that owns it or is narrative of the walk, its caveat is gone, and its gap list holds only gaps open after this Pull Request, each naming what would close it → Test `test_lease_flow_links_its_owners`
- **Dimension 3.5** — No page under `docs/architecture/` names Zig, `zlint` or NullClaw; only `docs/v2/done/` and `docs/v2/pending/` keep them as history → Test `test_architecture_names_no_zig`

### §4 — The published docs and the changelog say what shipped

On `chore/m213-rust-runner-changelog` in `~/Projects/docs`: the runner page carries the three telemetry knobs and the changelog entry M214_001 deferred to this cutover (draft at `~/.gstack/projects/agentsfleet-agentsfleet/reports/m214_docs_draft_Oct_06_2026.md`), with this cutover's own runner-page changes; the tools page carries the full catalog with the names the harness added (`exec_command`, `write_stdin`, `apply_patch`, `update_plan`, `wait_agent`, `send_input`, `list_agents`, `interrupt_agent`), drops `calculator`, which the Rust runner does not host (Indy, Oct 03: "Cut it now"), and drops none the runner still carries; the runner install page describes the binary, the signed toolbox and the offline bundle, the toolbox's admission and the `/dev/kvm` probe; `snippets/rates.mdx` stops naming a Zig file; a changelog `<Update>` states the cutover in the changelog's voice.

- **Dimension 4.1** — The docs branch carries the four page changes and the entry, and their checks pass → Test `test_docs_branch_carries_cutover_pages`

### §5 — Rollback is the previous release

The previous release's `agentsfleet-runner-linux-amd64` (Zig) stays on the release page; redeploying it through `deploy-dev-metal.yml` with that artifact is the rollback, and the daemon keeps accepting a runner that sends no outcome fields and no trace (M209_001 Dimension 1.2). A drill proves it once on dev.

- **Dimension 5.1** — Redeploying the previous Zig artifact on dev runs a lease; redeploying the Rust artifact runs it again → Test `test_rollback_drill_on_dev`

### §6 — The bare-metal unit can host the Rust runner

`deploy/baremetal/agentsfleet-runner.service` was written for the Zig runner. Under `ProtectSystem=strict` its `ReadWritePaths` leaves the storage home `/var/lib/agentsfleet-runner` read-only, so the runner fails at boot; `Delegate=cpu memory pids` omits `io`, which the sandbox probe requires; and every host publishes metrics under one identity. The unit gains the storage home in `ReadWritePaths`, `io` in `Delegate=`, and `Environment=OTEL_SERVICE_INSTANCE_ID=%H`. The Pull Request states how an operator rolls the unit out on a host.

- **Dimension 6.1** — The unit's `ReadWritePaths` holds the storage home and `Delegate=` holds `io`; `systemd-analyze verify` accepts it → Test `test_baremetal_unit_hosts_the_rust_runner`
- **Dimension 6.2** — Each host publishes its own metric series: `OTEL_SERVICE_INSTANCE_ID=%H` reaches `service.instance.id` → Test `test_runner_instance_id_from_unit`

### §7 — `apply_patch` never writes a file it cannot read back

Carried from #732's review (greptile `discussion_r4198887936`). `apply_patch` plans every hunk over an in-memory overlay with no cap, then lands each update by re-reading its file through `whole()`, which refuses a file longer than `MAX_READ_BYTES` (8 MiB). A patch whose first update grows a near-cap file past 8 MiB and whose second edits it again lands only the first. `plan()` now refuses any Add or Update whose resulting text is longer than `MAX_READ_BYTES` with `ToolErrorCode::FileTooLarge`, before anything is written, worded as `whole()` words it through one shared constant. `MAX_READ_BYTES` stays the one bound. Out of scope: how `land()` re-reads, the summary's counts, partial-landing wording, moves between a link and its target.

- **Dimension 7.1** — The greptile repro (a `MAX_READ_BYTES - 20` file, a 40-byte insert, then `tail` → `TAIL`) is refused and the file is byte-identical → Test `test_chained_patch_past_read_cap_is_refused_whole`
- **Dimension 7.2** — One update that grows the file past the cap is refused and the file is unchanged → Test `test_update_past_read_cap_is_refused`
- **Dimension 7.3** — A result of exactly `MAX_READ_BYTES` lands, and a later hunk on it in the same patch lands too → Test `test_update_to_exactly_read_cap_lands`
- **Dimension 7.4** — Through a link: the first hunk grows `keep.txt` past the cap, the second edits `alias.txt` → `keep.txt`; refused, nothing landed → Test `test_link_alias_past_read_cap_is_refused`

## Interfaces

```
Release artifacts:  agentsfleet-runner-linux-amd64 · toolbox-<sha256>.erofs · its manifest + signature · SBOM · scan report · offline bundle
Metal install:      <runner dir>/agentsfleet-runner · <runner dir>/toolbox/staging/ → the runner publishes <runner dir>/toolbox/<sha256>.erofs
Lanes after:        make test-unit-all · make lint-all · make test-integration-rustd · make test-runner-kernel · make check-version
Architecture check: no page describes a NullClaw child as the workload outside a Decisions or history row
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Rust runner misbehaves on dev | A bundle fails on dev metal | Fix on the branch; the Pull Request does not merge; nothing deleted yet reaches `main` |
| Rust runner misbehaves after release | A fleet fails in production | Redeploy the previous release's Zig artifact (Dimension 5.1); the daemon accepts both |
| Toolbox missing on a host | Deploy copied one of two | The runner's capability report says so; it refuses leases; the deploy job is red |
| Toolbox signature does not verify | Wrong key, or the artifact changed after signing | The runner refuses to admit it; the capability report says no toolbox; the deploy job is red |
| Grype finds a fixable Critical | A Debian package needs a newer snapshot | The toolbox job is red until the snapshot moves or Debian's tracker marks it not affected |
| A lane still needs Zig | A missed make dependency | Dimension 2.2 fails in CI on a runner without Zig |
| A comment reintroduces Zig | Review miss | Dimension 3.1's grep fails in the unit lane |
| Docs branch out of step | Published page lists a tool the runner lacks | Dimension 4.1's docs checks fail on that branch |

## Invariants

1. One runner is built, tested and deployed, and it is the Rust one (Dimensions 1.1, 2.1, 2.2).
2. The repository's code and current architecture describe no Zig runner and no NullClaw child (Dimensions 3.1, 3.2).
3. The previous release remains a working rollback until the next release supersedes it (Dimension 5.1).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes; the runner's own events arrived with M210–M212 | not applicable | — | — | — | `test_dev_runner_registers_with_capabilities` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | manual | `test_release_artifacts_are_static_and_addressed` | release run → zero `NEEDED` for the binary; toolbox name equals its SHA-256; `cosign verify-blob` with the release public key passes; SBOM and bundle attached; run URL in Session Notes |
| 1.2 | manual | `test_dev_runner_registers_with_capabilities` | dev deploy → `agentsfleet runners list` shows the runner with `kvm` and `toolbox_fs` fields; output pasted |
| 1.3 | manual | `test_reference_bundles_run_on_dev` | one mention, one repairer line, one `pull_request`, one `workflow_run` → four settled threads; links pasted |
| 1.4 | integration | `test_runner_binary_runs_a_lease` | the built binary against a fake daemon granting one lease → one settled report and no `run_refused` line |
| 2.1 | unit | `test_no_zig_sources_remain` | `git ls-files '*.zig' build.zig build.zig.zon build_runner.zig` → empty |
| 2.2 | integration | `test_lanes_need_no_zig` | `PATH` without `zig` → `make test-unit-all` and `make lint-all` exit 0 |
| 2.3 | unit | `test_check_version_reads_two_manifests` | `make check-version` → exit 0; no `build.zig.zon` in its output |
| 2.4 | unit | `test_repository_config_names_no_zig` | grep over `.github`, `make`, `Makefile`, `scripts`, `.oracle/orly.json` → 0 hits |
| 3.1 | unit | `test_rust_tree_names_no_zig` | `grep -rci 'zig\|nullclaw' rustd/crates --include='*.rs'` → every count 0 |
| 3.2 | unit | `test_architecture_describes_the_rust_runner` | `scripts/check_architecture_doc_test.sh` → the new check passes on `main`, fails on its fixture |
| 3.3 | unit | `test_cli_comments_point_at_rust` | grep `\.zig` over `cli/src` → 0 hits |
| 4.1 | manual | `test_docs_branch_carries_cutover_pages` | docs Pull Request link; its checks green; pasted |
| 1.5 | integration | `test_e2e_runner_lease_trace_reaches_a_collector` | the built binary leases from a fake daemon → the collector receives the lease's spans |
| 3.4 | unit | `test_lease_flow_links_its_owners` | `scripts/check_architecture_doc.sh` exit 0; `grep -c 'not reconciled\|Read this first' docs/architecture/lease_flow.md` → 0 |
| 3.5 | unit | `test_architecture_names_no_zig` | `grep -rliE 'zig\|zlint\|nullclaw' docs/architecture` → no file |
| 6.1 | unit | `test_baremetal_unit_hosts_the_rust_runner` | `systemd-analyze verify` on the unit → exit 0; `ReadWritePaths` names `/var/lib/agentsfleet-runner`; `Delegate=` names `io` |
| 6.2 | unit | `test_runner_instance_id_from_unit` | `OTEL_SERVICE_INSTANCE_ID=host-a` → the exported resource carries `service.instance.id=host-a` |
| 7.1 | unit | `test_chained_patch_past_read_cap_is_refused_whole` | real executor (`crate::testing::Live`) → `FileTooLarge`, path in the text, file byte-identical |
| 7.2 | unit | `test_update_past_read_cap_is_refused` | → `FileTooLarge`, file unchanged |
| 7.3 | unit | `test_update_to_exactly_read_cap_lands` | result of exactly `MAX_READ_BYTES` → lands; a second hunk lands too |
| 7.4 | unit | `test_link_alias_past_read_cap_is_refused` | `src/alias.txt` → `keep.txt` → `FileTooLarge`, nothing landed |
| 5.1 | manual | `test_rollback_drill_on_dev` | Zig artifact redeployed → a lease settles; Rust redeployed → a lease settles; both thread links pasted |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No Zig source, build file or reference remains (§2, §3) | `git ls-files '*.zig' build.zig build.zig.zon build_runner.zig \| wc -l; grep -rci 'zig\|nullclaw' rustd/crates --include='*.rs' \| grep -v ':0$' \| wc -l` | `0` and `0` | P0 | |
| R2 | The lanes pass without Zig (§2) | `env PATH="$(echo "$PATH" \| tr ':' '\n' \| grep -v zig \| paste -sd: -)" make test-unit-all && make lint-all` | exit 0 | P0 | |
| R3 | The architecture check guards the pages (§3) | `bash scripts/check_architecture_doc_test.sh && bash scripts/check_architecture_doc.sh` | exit 0 | P0 | |
| R4 | Workflows approved and the release ships the static binary, the signed toolbox, its SBOM and the offline bundle (§1) | manual — Indy approves each workflow edit by name; evidence: the release run URL and the verify step's output in Session Notes | approval quotes and a green run URL | P0 | |
| R5 | The dev runner serves the four bundles and the rollback drill passes (§1, §5) | manual — four thread links and two drill thread links in Session Notes | six links | P0 | |
| R6 | Docs branch open with the cutover pages (§4) | manual — the docs Pull Request link in Session Notes | link, checks green | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** `src/runner/`, `src/lib/`, `src/build/`, `build.zig`, `build.zig.zon`, `build_runner.zig`.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| Zig build and output | `grep -rn "zig-out\|build_runner\|zig build\|\.zig-cache" --include='*.mk' --include='*.yml' --include='*.sh' --include='Makefile' . \| grep -v node_modules` | 0 matches |
| NullClaw | `grep -rni "nullclaw" --include='*.rs' --include='*.ts' --include='*.mk' --include='*.yml' --include='*.zon' . \| grep -v node_modules` | 0 matches |
| `RUNNER_ZIG_VERSION`, `ZIG_LOCAL_CACHE_DIR`, `ci-zig-` | `grep -rn "RUNNER_ZIG_VERSION\|ZIG_LOCAL_CACHE_DIR\|ci-zig-" . --include='*.mk' --include='*.yml' --include='Makefile'` | 0 matches |

## Out of Scope

- Firecracker, workspaces in R2, the outage toolkit — they ride the Rust runner after it is the only one.
- Deleting the `ci-zig-alpine` and `ci-zig-ubuntu` images from the registry — an operator step, recorded in Session Notes when done.
- Revoking a compromised toolbox — deferred (Discovery). The daily scan, weekly refresh and 24-hour emergency release `runner_execution.md` §Toolbox sets are run by an operator until a scheduled job is specced. An arm64 release, dm-verity and the Firecracker boot artifact wait on their own specs and M211_001's spikes.
- Rewriting dated history: Decisions rows, changelog entries and done specs keep their quotes.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet owner notices nothing on the day, then opens a thread and sees Codex-style cells, a `Ran bun test` row and a diff where before there was a name and a clock.
2. **Preserved user behaviour** — Every bundle keeps running; the daemon's verbs are unchanged; the previous release stays a one-command rollback.
3. **Optimal-way check** — Deploy from the branch, prove on dev, delete in the same Pull Request, so the proof is never older than the deletion.
4. **Rebuild-vs-iterate** — The deletion half of Indy's "fresh port".
5. **What we build** — A static binary and a signed, scanned toolbox with its offline bundle, a staged metal install, one architecture check, one docs branch, one changelog entry.
6. **What we do NOT build** — New runner capability (see Out of Scope).
7. **Fit with existing features** — The release and deploy pipelines keep their artifact names where they can, so the metal lane's shape survives.
8. **Surface order** — Release first; docs branch in the same window.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — The runner install page names the two files and the probe; `agentsfleet runners list` shows what a host reported.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Pull Request that switches the build, proves on dev, and deletes, because two runners on `main` for longer means two sets of lanes, images and comments to keep honest.
- **Alternatives considered:** a strangler period with both runners deployed (rejected by Indy: "The port is a fresh port, since we always have the last binary with us and running" — the rollback is the artifact, not a second deployment); deleting Zig in a later Pull Request (rejected: the deletion is what proves nothing depends on it).
- **Patch-vs-refactor verdict:** this is a **refactor** because it removes a build system and a binary.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "The port is a fresh port, since we always have the last binary with us and running."; "A second copy of the wire will not be existing, none of the rust code will point to the zig."; "the rust code is independent and follows our current rustd/ principles". The 510-file, 1,684-line count is from `grep -rli 'zig\|nullclaw' rustd/crates --include='*.rs'` on Oct 02, 2026.
- **Toolbox review** — Tarzy reviewed the toolbox design on Oct 03, 2026; Indy chose "Approve as classified (Recommended)". This spec carries signing, the SBOM and scan, and eager distribution with an offline bundle; M211_001 carries the pinned build and admission (`runner_execution.md` §Toolbox, Decisions).
- **Required human decisions** — Indy's explicit approval of each of the eight workflow edits (R4), and the release itself (`gh release create` is his).
- **Credentials** — the toolbox release key pair: the private half a CI secret the signing step reads, the public half built into the runner as `TOOLBOX_RELEASE_PUBLIC_KEY`. Both exist and are named before §1 starts.
- **Carried from M210_002** — review P1-7: `agentsfleet_runner/src/main.rs:134` refuses every lease, so M210_002's §1–§4 run nowhere in production until this spec switches the engine on. > Indy (2026-10-04 08:51): "Defer to M213 (Recommended)" — context: Dimension 1.4 here.
- **Metrics review** — No analytics or funnel playbook update required.
- **CHORE(open) amendments** — Indy (in-session, Oct 06, 2026) folded four items into this spec: the cutover composing `afr_supervisor::run` with telemetry end to end (Dimension 1.5); the bare-metal unit, "I approve these deploy-file edits for this workstream" (§6); the M214_001 docs deferred to the cutover (§4); and the lease-flow page review (Dimension 3.4). The `~/Projects/docs` write is approved for this workstream on its own `chore/m213-…` branch. The sandbox allowlist is M213_002, in this Pull Request. Indy (Oct 07, 2026), on the `apply_patch` read-cap regression from #732: "in your M213* fixes ensure this is done too" (§7). Indy (Oct 07, 2026): "ensure all zig, zlint, and other reminscent (so no zig must appear i think) including in @docs/architecture/, you can keep the zip in @docs/v2/done and in pending, since they are legacy" (Dimension 3.5); "Ensure the CI gates are updated for the deploy of the rust binary" (the workflow edits of §1 and §2, R4); "Ensure the make targets (memleak, lint-zig* or test-unittest-zig* are all nuked)" (§2); "ensure the rust standards for trait, Fn callables are used" (every new Rust signature).
- **Skill-chain outcomes** — pending.
- **Deferrals** —
  > Indy (2026-10-03 13:51): "I dont want to focus on revocation of a compromised toolbox, first is to get the toolbox working" — context: revoking a compromised toolbox digest, from Tarzy's review; left out of this spec and M211_001.
