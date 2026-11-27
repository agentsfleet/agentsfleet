<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M258_003: Production runs on Firecracker, runner tiers name engines, and bubblewrap is gone

**Prototype:** v2.0.0

**Milestone:** M258

**Workstream:** 003

**Date:** Nov 27, 2026

**Status:** IN_PROGRESS

**Priority:** P1, a shared-kernel engine still ships beside the VM engine, and the runner tiers still name Linux mechanisms

**Categories:** API, CLI, DOCS, INFRA, OBS

**Batch:** B1, in the one M258 Pull Request, built after M258_001 and before M258_002 (Indy's D22 ruling)

**Depends on:** M258_001, for the seam, the conformance suite, the Firecracker engine and the dev host on Firecracker

**Provenance:** rewritten by Aiwa (Claude Opus 5.5, Nov 27, 2026) from Indy's rulings of Nov 26, 2026, which moved the Sprites engine, the engine policy, burst, a Sprites rate and the macOS runner to M260_001. The earlier draft's Sprites sections are M260_001's to carry.

**Canonical architecture:** `docs/architecture/runner_execution.md` §Sandbox engines, `docs/architecture/runner_fleet.md` §Placement; this spec rewrites both for one engine

---

## Overview

**Goal (testable):** Every production runner reports `engine: firecracker`, the runner tiers are `firecracker` and `dev_none`, and no code, deploy file or workflow names bubblewrap.

**Scope, after Indy's rulings.** M258 is the compute layer in one Pull Request. This Workstream switches production to Firecracker, renames the tiers for engines, and deletes bubblewrap. The Sprites engine and everything that needs a second engine wait for M260_001.

**What changes for a fleet.** Each line is a behaviour Indy accepts by approving this spec.

1. Production switches to Firecracker when the M258 Pull Request merges, with no bake period (D12, Indy: "we are not in production yet, so we switch on merge of firecracker and test in the dev box").
2. A runner still on bubblewrap is refused at hello, because the runner protocol `VERSION` moved and the engine is gone.
3. Billing is unchanged: every run is billed at today's rate.

## PR Intent & comprehension handshake

- **PR title (eventual):** carried by M258_001's title for the one M258 Pull Request.
- **Intent (one sentence):** The shared-kernel engine is gone, and every runner names the VM engine it runs.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/runner/afr_engine_bubblewrap/**` | DELETE | §3 |
| `rustd/crates/runner/main/**` | EDIT | `EngineChoice::Bubblewrap` and its feature go; the kernel lane defaults to Firecracker |
| `rustd/crates/runner/{afr_engine,afr_supervisor,afr_cgroup,afr_guest,afr_image,afr_firewall}/**` | EDIT | the bubblewrap-only paths and names go |
| `rustd/crates/protocol/af_wire/src/runner/{mod.rs,capability.rs}`, `af_wire/src/report/mod.rs`, `af_wire/src/protocol/mod.rs` | EDIT | tiers `firecracker` and `dev_none`; the report's five Linux booleans and `LandlockDeny` go; `VERSION` moves once with M258_001's move |
| `rustd/crates/daemon/afd_runner/src/{reconcile.rs,spelling.rs,view/**}` | EDIT | the evidence table reads engines; a tier whose report names another engine reads degraded |
| `rustd/crates/daemon/afd_db/schema/*runner*` | EDIT | tier spellings live in the application, never a `CHECK`; edited in place before production |
| `cli/src/commands/platform/{runners.ts,runner-vocabulary.ts,runner-schema.ts}`, `cli/test/**` | EDIT | the tiers and the report's fields |
| `deploy/baremetal/agentsfleet-runner.service`, `deploy/baremetal/toolbox.sh`, `playbooks/lib/runner/{prepare.sh,common.sh}`, `playbooks/lib/egress_host_deps.sh` | EDIT | `--engine firecracker`, the Firecracker and jailer binaries, no bubblewrap install |
| `deploy/local/**`, `Dockerfile`, `scripts/build-acceptance.ts`, `make/test-unit.mk` | EDIT | no bubblewrap tier, package or engine argument |
| `.github/workflows/{build-candidate.yml,release.yml,acceptance-on-demand.yml}`, `.github/actions/**` | EDIT, with Indy's approval at build | production's engine flag; acceptance opens `/dev/kvm` on GitHub's runner with a udev rule; bubblewrap names go |
| `docs/architecture/{runner_fleet,runner_execution,testing}.md`, `docs/AUTH.md`, `docs/development.md`, `SECURITY.md`, `playbooks/PRIME.md` | EDIT | one engine; SMT on everywhere and its accepted side-channel risk (Indy: "On everywhere") |
| `~/Projects/docs` self-hosting pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | a runner host needs usable KVM |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FEATURE and RS-COMBOS for the engine features. **`code.md`** — NLG, ORP, dead code removed whole. **`domains/sql.md`** — STS for spellings bound from constants. **`tests.md`** — TEST-NEG.

## Sections (implementation slices)

### §1 — Production runs on Firecracker

Production runner hosts start with `--engine firecracker` from the release that ships M258. Each host is assumed to match the dev host: Debian 13, its stock kernel, `/dev/kvm` usable, SMT on (Indy: "just make assumptions around how you saw in dev machine = same kernel debian 13, kvm"). Boot refuses a host without usable KVM, naming it, as M258_001 §7 built.

- **Dimension 1.1** — open: needs Indy's approval for the deploy and workflow edits — production reports Firecracker → Test `production_runners_report_firecracker` (manual, after merge). Every production runner row shows `engine: firecracker`, `kvm: usable` and a self-test `all_ok`; evidence goes in the Pull Request's follow-up note.

### §2 — Tiers name engines

`SandboxTier` becomes `firecracker` and `dev_none`; `landlock_full` and `container_nested` go. The reconcile demands that a runner's reported `engine` match its tier, and a mismatch reads degraded with its own reason.

- **Dimension 2.1** — ✅ DONE (unit) — a tier names an engine → Test `test_a_tier_names_its_engine` (unit, `af_wire` and `afd_runner`). `firecracker` and `dev_none` round-trip, and `landlock_full` fails to decode. A `firecracker` tier whose report names `unsandboxed` reads degraded with its own reason.

### §3 — bubblewrap deleted

`afr_engine_bubblewrap`, `EngineChoice::Bubblewrap` and its feature go, with the report's `landlock`, `seccomp`, `cgroup_controllers`, `bubblewrap` and `egress_enforcement` fields, `FailureClass::LandlockDeny`, the bubblewrap install and the shared-kernel `Isolation` value if nothing else reports it. `afr_guest`'s hardening stays: the Firecracker guest's inner environment runs it.

- **Dimension 3.1** — ✅ DONE (unit) — no engine named bubblewrap → Test `test_bubblewrap_is_not_an_engine` (unit, `main`). `--engine bubblewrap` answers `InvalidValue`; `EngineChoice` holds `firecracker`, plus `unsandboxed` in debug builds.
- **Dimension 3.2** — ✅ DONE (unit) — the report names no Linux mechanism → Test `test_a_report_naming_a_linux_mechanism_is_refused` (unit, `af_wire`). A report carrying `landlock` is refused as an unknown field; an engine report round-trips.
- **Dimension 3.3** — wired, not yet run on a KVM host — the lane runs Firecracker by default → Test `make test-runner-kernel` (integration, a KVM host). With no `ENGINE`, the suite runs on Firecracker, and every trial passes.

**What §2 and §3 built.** `SandboxTier` is `firecracker` or `dev_none`; a `firecracker` tier needs a report naming `engine: firecracker` with `hardware_vm`, and any other reads degraded with `REASON_ENGINE_NOT_THE_TIERS`. `afr_engine_bubblewrap`, the `sandbox` subcommand, the report's Linux booleans, `LandlockDeny` and `Isolation::SharedKernel` are deleted; `--engine unsandboxed` replaces `--unsandboxed` in debug builds. The kernel lane boots through `FirecrackerBackend` as a host does, and its real-host trials were ported to the seam. With `egress_enforcement` gone, an `allow_list_egress` runner is proven by its engine alone: Firecracker reads every plan back in force before a build returns, so a host that cannot enforce fails its builds closed. Open: the deploy files, `Dockerfile`, workflows and playbooks that still name bubblewrap wait on Indy's approval; `deploy/local/bootstrap.ts` still assigns `container_nested`.

## Interfaces

```text
Wire     SandboxTier: firecracker | dev_none;  CapabilityReport: engine, isolation, kvm, sandboxes_max (+ durable_tasks_v1, build_id)
Runner   agentsfleet-runner run --engine firecracker   (unsandboxed in debug builds)
Make     make test-runner-kernel  (ENGINE defaults to firecracker)
```

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Host without KVM | failure | a production host lacks `/dev/kvm` | boot refuses naming `kvm`; the runner stays down | M258_001 `test_firecracker_refuses_a_host_without_usable_kvm` |
| Tier and engine disagree | failure | an operator assigns `firecracker` to a debug runner | degraded with its own reason | `test_a_tier_names_its_engine` |
| Old report shape | failure | a runner sends the Linux booleans | refused as an unknown field | `test_a_report_naming_a_linux_mechanism_is_refused` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every open owner decision is ruled (Discovery) | `git grep -n -E "^  - \*\*[A-Z][0-9]+\*\* .\[\?\]" -- "docs/v2/*/M258_003_*"` | no output | P0 | |
| R2 | bubblewrap is gone | `git grep -c -w -i bubblewrap -- rustd cli deploy make scripts playbooks/lib Dockerfile .github` | no output | P0 | |
| R3 | Tiers name engines (§2) | `make test-unit-all` | exit 0; `test_a_tier_names_its_engine` passes | P0 | |
| R4 | The lane defaults to Firecracker (§3) | `make test-runner-kernel` | exit 0; zero failed trials | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the bubblewrap engine | `git ls-files rustd/crates/runner/afr_engine_bubblewrap` | no output |
| the Linux-named tiers and fields | `git grep -n -w -e landlock_full -e container_nested -e LandlockFull -e ContainerNested -e LandlockDeny -e egress_enforcement -- rustd cli ui/packages deploy public` | no output |

## Out of Scope

- Fly.io Sprites, the engine policy and its tenant route, CLI and page, burst, a Sprites rate and the macOS runner: M260_001 (Indy: "I prefer we move the sprites to M260 milestone_001").
- Checkpoint and snapshot on the chunk map: M258_002, built after this Workstream.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "Also i think i prefer we move the sprites to M260 milestone_001 But the M259_003 becomes what ever we had sans sprites as a new engine" and "since sprites as a new engine is a separate scope doesnt yield any benefit"; asked again, chose "Production switch + delete bubblewrap (Recommended)" — context: this Workstream's scope; billing untouched.
- > Owner (Indy, Nov 26, 2026): chose "No bake" and "we are not in production yet, so we switch on merge of firecracker and test in the dev box you just jumped in." — context: §1 switches production at merge (D12).
- > Owner (Indy, Nov 26, 2026): "also i would say i havent though about this in production, so in M258_003 just make assumptions around how you saw in dev machine = same kernel debian 13, kvm that you made it work with may be better ram, cpu cores and nvme disks. so you can assume that." — context: D13 and A2, production hosts assumed to match the dev host.
- > Owner (Indy, Nov 26, 2026): chose "On everywhere" for SMT — context: production keeps SMT on; `SECURITY.md` records the cross-VM side-channel risk as accepted.
- > Owner (Indy, Nov 26, 2026): chose "One PR for all of M258" and "Delete bubblewrap before M258_002 (Recommended)" — context: this Workstream ships in the one M258 Pull Request, built second.
- **Open owner decisions** — none.
- > Owner (Indy, Nov 27, 2026): "critical cve fail the build, i think you already do that for the toolbox" — context: `.github/actions/scan-guest/action.yml` fails when the pinned guest kernel is behind a Critical fix on its own line; the remedy is a new version and SHA-256 in `scripts/toolbox/guest.txt`.
- > Owner (Indy, Nov 27, 2026): "Chromium sandbox - what is a quick win with trait based abstraction to improve and test later? is what i want you to build" — context: the tenant filter refuses `clone` with a namespace flag and the guest kernel has no user namespaces (`afr_guest/src/harden/linux.rs:38-71`, `scripts/toolbox/guest.config`), so Chromium cannot build its renderer sandbox in the microVM. M258 builds the seam only: a process is confined by a `Confinement` the executor chooses, with the tenant's as its one implementation. M259_001 adds the browser's, whose choices are `--no-sandbox`, namespaces for Chromium alone, or a second microVM.
- > Owner (Indy, Nov 27, 2026): "ignore any depdup for now, but have a trait provide which does noop but activated later on the implementation side after testing" — context: an export packs a second name of one file as a link, but the host's scan reads every name in full and a restore writes each as its own file (`afr_supervisor/src/recovery/catalog.rs:157`). M258_002 §3 puts that choice behind a trait whose one implementation treats every name as its own file; recording a link by its inode is a later implementation.
- > Owner (Indy, Nov 27, 2026): chose "Yes, in this PR" for Bun in the toolbox, then "For 2 - raise to 195MB" — context: the toolbox vendors Bun as it vendors `uv`, and its size cap moves from 162,004,991 to under 195,000,000 bytes. Then "Node must be nukedd" — context: `nodejs` leaves the toolbox; Bun is its one JavaScript runtime and answers to the name `node`. Then "Move the python , and so on to taltest", "i think move python to 3.15, remove the python 3.13" and "if node is removed then the size must come down" — context: the Debian snapshot, `uv` and Bun move to their latest releases; the image's new size is reported for a lower cap, which is Indy's to name. Then "I think stick to python 3.13" — context: Debian stable ships only 3.13.5 and a standalone 3.15 costs size, so Debian's `python3` stays. Then "also ensure the latest jq, curl, python3.13 all are updated to the latest", "ripgrep ca-certificates too", and chose "Small set, no compiler (Recommended)" — context: every Debian package in the toolbox is the newest trixie serves at its snapshot, and `procps`, `unzip`, `xz-utils`, `bzip2`, `zstd`, `file`, `patch`, `make` and `tree` join it; no C compiler.
- > Owner (Indy, Nov 27, 2026): "Rename all five with these inputs - what is Dialer and Listener make a better name?" and "Yes rename" — context: `Enforcer` is `EgressEnforcer`, `Failure` is `EngineFailure`, `Stream` is `ExecutorStream`, `afr_executor`'s `Transport` is `ExecutorRoute` (it collided with `af_egress::Transport`), and the guest's `Accept` is `PeerConnections`; the supervisor's `hold` modules and types say `park`.
- **Deferrals and transfers** — the Sprites engine and everything that needs a second engine, to M260_001, by Indy's quote above.
