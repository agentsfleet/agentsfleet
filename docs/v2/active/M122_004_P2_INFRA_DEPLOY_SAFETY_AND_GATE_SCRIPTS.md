<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M122_004: Deploy script version/lock safety and repo gate scripts that actually fire

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 004
**Date:** Jul 09, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — operator-tooling hardening: a deploy skip on a substring version collision (visibly-stale binary), an unserialized deploy that can overlap on manual/orphaned runs, two credential-rotation scripts missing the vault approval+auth friction their siblings enforce, and two doc-freshness gates that silently pass (one hardcodes a frozen milestone range and never runs; one is blind to underscore-named targets). No user, data, or runtime blast radius — the failures are stale-artifact and weakened-guardrail shaped, not blocking or exploitable.
**Categories:** INFRA
**Batch:** B1 — runs alone; touches deploy + playbooks + scripts + one make file, no overlap with other pending work.
**Branch:** `feat/m122-deploy-safety-gates`
**Test Baseline:** unit=2402 integration=267
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, Jul 02, 2026 `fleet-wide-refactor-audit`; every finding re-verified against HEAD 7a06fb5d on Jul 09, 2026 by the `audit-open-items-recheck` workflow, each surviving an adversarial refutation pass with severities corrected down from the original audit).
**Canonical architecture:** `docs/architecture/direction.md` — platform determinism + gate discipline; the deploy/vault conventions these fixes conform to live in `playbooks/lib/common.sh` and `docs/REST_API_DESIGN_GUIDELINES.md` §7.

---

## Overview

**Goal (testable):** `deploy.sh` skips a redeploy only when the installed binary's exact version token equals the target (never a substring), refuses to run while another deploy holds a lock; both `credential_rotation` scripts refuse to read the vault without `ALLOW_VAULT_READS=1` and an authenticated `op`; `check_architecture_doc.sh` validates every milestone identifier (no frozen range) and runs inside `make lint-all`; and `check_route_registration_doc.py` sees underscore-named make targets in both its citation and definition scans.

**Problem:** `deploy/baremetal/deploy.sh:77` uses a bash glob substring match (`== *"${VERSION#v}"*`), so a stable `v0.1.0` over an installed `0.1.0-rc1`, or a `v0.1` tag over an installed `0.10.x`, wrongly reports "already installed" and skips the redeploy; `main()` at line 237 runs install + `systemctl restart` with no mutex, so a manual prod run or a cancel-orphaned Continuous Integration (CI) run can overlap a second invocation. `credential_rotation/01_vault_sync.sh:35` and `02_service_health.sh:34` call `op read` directly without sourcing `../../lib/common.sh` or calling `playbooks_require_vault_read_approval` / `playbooks_require_op_auth`, which the sibling `ip_allowlisting` scripts do — a weakened operator guardrail on diagnostic scripts (no plaintext secret leaks; refs and checkmarks only). `check_architecture_doc.sh:32` hardcodes `grep -E "^M(40|...|51)"` while `docs/architecture` cites identifiers up to M121, so every M52+ reference is silently skipped, and the script is wired into no make target so it never runs. `check_route_registration_doc.py` lines 60/64 use `[a-z][a-z0-9-]*` char classes that exclude underscore, so an underscore-named target cited in the Representational State Transfer (REST) guide is invisible in both directions — a latent false-negative in a live gate.

**Solution summary:** These are gates and safety checks that fail to fire or fire too loosely; per `AGENTS.md` the sanctioned direction is to repair the check, not silence it. Parse the exact version token from `--version` and compare with `==`; wrap `deploy.sh main()` in `flock -n` on a fixed lock path and die when held. Source `common.sh` and call both vault gates at the top of the two `credential_rotation` scripts, mirroring `ip_allowlisting`. Drop the frozen milestone alternation for a validate-every-identifier scan and wire the script into a new `make check-architecture-doc` target inside `lint-all`. Widen both regex char classes to include underscore and add a live regression citation of a real underscore target to the REST guide.

## PR Intent & comprehension handshake

- **PR title (eventual):** Harden deploy version/lock safety and repair two doc-freshness + one vault-approval gate
- **Intent (one sentence):** an operator's redeploy is skipped only on a true version match and never overlaps itself, vault-reading rotation scripts carry the same approval friction as their siblings, and the architecture-doc and route-registration gates actually catch drift instead of silently passing.
- **Handshake** (filled at PLAN, Jul 10, 2026 — before any edit)

  **Intent, restated.** Five safety checks in this repo either fire too loosely or never fire at all. Two live in the deploy path: a redeploy is skipped whenever the installed binary's version *contains* the target as a substring, so `v0.1.0` over `0.1.0-rc1` leaves a stale binary running and reports success; and nothing serializes `main()`, so two operators — or an operator and a cancel-orphaned Continuous Integration (CI) job — can interleave `install` and `systemctl restart`. One lives in the vault path: the two `credential_rotation` scripts read 1Password without the approval prompt and `op`-auth pre-check that every sibling operations script enforces. Two are repo gates that pass while proving nothing: the architecture-doc checker only validates milestone identifiers M40 through M51 (a range frozen years of milestones ago) and is wired into no make target, so it never even runs; and the route-registration checker's make-target regexes cannot see an underscore, so an underscore-named target cited in the guide is invisible to both its citation scan and its definition set. After this work an operator's redeploy either truly skips or truly reinstalls and never overlaps itself, a rotation script refuses the vault until approval and sign-in are both present, and both doc gates actually catch drift. I am repairing the checks, not silencing them.

  **ASSUMPTIONS I'M MAKING:**

  1. A redundant reinstall is always safe; a wrong skip is the bug. Every ambiguous `--version` shape — empty output, a single field, a read error — therefore resolves to "not installed" and the deploy proceeds.
  2. `deploy.sh` runs only on Linux bare metal, so `flock` is available at deploy time. Its absence at *test* time (macOS) is a test-harness concern, not a deploy concern, and is handled per Indy's call in Discovery.
  3. The lock must be injectable for the test to exercise it: `/var/lock/` is not writable by a non-root test process. The lock path stays a single named constant with an environment override, defaulting to `/var/lock/agentsfleet-deploy.lock`.
  4. Sourcing `deploy.sh` from the test must not execute a deploy. The `main "$@"` call is guarded by a sourced-vs-executed check so the functions are reachable without side effects.
  5. The two `credential_rotation` scripts keep their `op_read_with_retry` wrapper untouched. §3 is additive — the gates go in front of the first read, not through a rewrite of the retry/cache logic. De-duplicating that wrapper is named in Out of Scope.
  6. `roadmap.md` is the only architecture doc whose job is naming unshipped work, so it is the only file where a `pending/` spec resolves. This is Indy's call, recorded in Discovery, not my inference.
  7. The `check-playbooks` parity grep binds `playbooks/operations/**` only. `playbooks/founding/**` is explicitly Out of Scope, so a founding script reading the vault does not fail this gate.
  8. Adding `check-architecture-doc` and `check-deploy-safety` to `lint-all` means both must be green on `main` the moment this lands — no new target may enter `lint-all` red. The `M105` carve-out exists precisely because of this.

  No mismatch with the Intent above; proceeding to EXECUTE.

## Implementing agent — read these first

1. `playbooks/operations/ip_allowlisting/01_egress_inventory.sh` (lines 4-6, 59-61) — the exact pattern §3 mirrors: resolve `SCRIPT_DIR`, `source "$SCRIPT_DIR/../../lib/common.sh"`, then call `playbooks_require_vault_read_approval` + `playbooks_require_op_auth` before the first read.
2. `playbooks/lib/common.sh` (lines 13-26) — the two gate functions §3 must call; do not reimplement them locally.
3. `scripts/check_route_registration_doc.py` (lines 60, 64, 104-116) — the two regexes §5 widens and the `real_make_targets` set-builder they feed; note the `_?` prefix already anchors underscore-led names, so only the inner char class needs `_`.
4. `scripts/check_architecture_doc.sh` (lines 31-51) — the frozen alternation §4 removes and the `done/`+`active/` resolution loop that stays; `make/quality.mk:198` (`check-route-registration-doc`) is the precedent target shape §4 mirrors.
5. `src/runner/cmd/version.zig` — the `agentsfleet-runner <version> (git <sha>)` output shape §1 parses (version is whitespace field 2); the deploy-side parse must degrade to "not installed" on any other shape.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/baremetal/deploy.sh` | EDIT | exact version-token equality (§1); `flock -n` mutex around `main()` (§2); guard the `main "$@"` call behind a sourced-vs-executed check so functions are unit-testable |
| `deploy/baremetal/deploy_test.sh` | CREATE | sources `deploy.sh`, exercises `is_already_installed` equality (§1) and lock mutual-exclusion (§2) |
| `playbooks/operations/credential_rotation/01_vault_sync.sh` | EDIT | source `common.sh` + call both vault gates before the first read (§3) |
| `playbooks/operations/credential_rotation/02_service_health.sh` | EDIT | same guardrail parity (§3) |
| `playbooks/operations/credential_rotation/vault_gate_test.sh` | CREATE | asserts both scripts block without approval / without `op` auth (§3); name is outside the `0[1-9]_`/`[1-9][0-9]_` gate glob (RULE GLS) |
| `playbooks/operations/observability/01_credentials.sh` | EDIT | reads the vault ungated — same `common.sh` preamble (§3). Added at EXECUTE per Indy's Jul 10 call; without it Dimension 3.3's operations-wide grep cannot pass. Also strips a milestone identifier from source (MSID / RULE NLR). |
| `playbooks/operations/observability/02_prometheus.sh` | EDIT | same ungated vault read, same preamble (§3) |
| `playbooks/operations/observability/03_dashboard.sh` | EDIT | same ungated vault read, same preamble (§3) |
| `scripts/check_architecture_doc.sh` | EDIT | validate every milestone identifier, drop the frozen `M(40..51)` alternation (§4) |
| `scripts/check_architecture_doc_test.sh` | CREATE | fixture-driven self-test for Dimensions 4.1/4.3 — `M999` fails everywhere, `pending/` resolves in `roadmap.md` only. Added at CHORE(open): §4 named the tests but gave them no file. |
| `scripts/check_route_registration_doc.py` | EDIT | widen both make-target char classes to include underscore (§5) |
| `scripts/check_route_registration_doc_test.py` | CREATE | asserts the widened regexes capture underscore targets and flag a phantom one (§5) |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | §7 gains one citation of a real underscore target (`make _fmt_check`) as a live regression fixture (§5) |
| `src/runner/cmd/version.zig` | EDIT | comment-only: the module doc comment and one test comment cite the substring check §1 deletes. No output-shape change (Out of Scope holds). Added to this table at CHORE(open) per Indy's Jul 10 call — see Discovery. |
| `make/quality.mk` | EDIT | new `check-architecture-doc` + `check-deploy-safety` targets in `lint-all`; `check-playbooks` gains the vault-gate parity grep + runs `vault_gate_test.sh`; `check-route-registration-doc` also runs its new test (§3/§4/§5) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (lock path `/var/lock/agentsfleet-deploy.lock`, the fixed field-2 delimiter, gate error messages → named `readonly`/module constants, one declaration site); **NDC** (delete the frozen `M(40..51)` alternation entirely, no commented-out residue); **NLR** (touch-it-fix-it on every edited script — remove any legacy framing in the same diff); **ORP** (grep the repo for the removed alternation string and the old char-class pattern before commit); **GLS** (the new `vault_gate_test.sh` must not match the `credential_rotation` gate glob); **TST-NAM** (companion test names carry no milestone identifiers); **FLL** (added shell functions and the Python test stay under the length caps).
- **`dispatch/write_any.md`** — cross-cutting authoring invariants for `*.sh` / `*.py`: File & Function Length, UFS named constants, MILESTONE-ID ban in the test identifiers.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` edited; §1 only reads `version.zig`'s output shape |
| PUB / Struct-Shape | no | no new pub/exported surface — shell functions and a private Python test |
| File & Function Length (≤350/≤50/≤70) | yes | new `deploy.sh` functions (lock acquire, version parse) and the test files stay well under the caps; split only if a single function nears 50 lines |
| UFS (repeated/semantic literals) | yes | lock path, version-field index, and repeated gate/error strings become named constants at one declaration site |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | RULE OBS scopes to `src/**/*.zig` + `agentsfleet/src/**/*.js`; shell `log`/`die` and gate stderr are unchanged in shape; no error-registry or schema surface |

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/operations/ip_allowlisting/01_egress_inventory.sh` — §3 is a byte-for-byte mirror of its source-and-gate preamble; divergence: the `credential_rotation` scripts keep their existing `op_read_with_retry` wrapper (the raw `op read` inside it now sits behind the two gates), rather than switching to `playbooks_read_ref_or_empty`.
- **Reference:** `make/quality.mk:198` `check-route-registration-doc` — the single-line `@python3`/`@bash` target shape §4's `check-architecture-doc` follows, added to the same `lint-all` prerequisite list.

## Sections (implementation slices)

### §1 — Deploy version check is exact, not substring

`is_already_installed` compares the running binary's `--version` output against the target with a bash glob substring test, so any version whose text merely contains the target skips the redeploy. Parse the exact version token (whitespace field 2 of the `agentsfleet-runner <version> (git <sha>)` line) and compare with `==` after stripping the `v` prefix. **Implementation default:** a malformed or unexpected `--version` shape resolves to "not installed" so the deploy proceeds — a redundant reinstall is safe; a wrong skip is the bug.

- **Dimension 1.1** — DONE — installed `0.1.0-rc1` with target `v0.1.0` (substring, not equal) → `is_already_installed` returns non-zero, the deploy reinstalls → Test `test_deploy_version_substring_not_equal_reinstalls`
- **Dimension 1.2** — DONE — installed version field equals the stripped target → `is_already_installed` returns zero, the deploy skips → Test `test_deploy_version_exact_match_skips`
- **Dimension 1.3** — DONE — a malformed `--version` shape (empty output or a line with no field 2) → `is_already_installed` returns non-zero (resolves to not-installed), the deploy reinstalls → Test `test_deploy_malformed_version_reinstalls`

### §2 — Deploy runs are mutually exclusive

`main()` runs install + `systemctl restart` with no serialization; a manual prod invocation or a cancel-orphaned CI run can overlap a second invocation of the non-atomic sequence. Wrap `main()` in `flock -n` on a fixed lock path and die immediately when the lock is held. **Implementation default:** a non-blocking lock (`-n`, fail-fast with a clear message) over a blocking wait — an operator wants to know a deploy is already running, not queue behind it.

**Portability (Indy's Jul 10 call — see Discovery):** `flock` ships with util-linux and is absent on macOS. `deploy.sh` targets Linux bare metal only, so it keeps `flock` — the kernel releases the lock when a `SIGKILL`ed deploy dies, which a `noclobber` lock file cannot do. `deploy_test.sh` therefore prints a loud `SKIP` for the two lock Dimensions when `flock` is absent, and hard-fails when `CI` is set — so the lock invariant is enforced on every `ubuntu-latest` runner and never silently green in CI.

- **Dimension 2.1** — DONE — with the lock held by another process, invoking the guarded entry exits non-zero immediately without installing → Test `test_deploy_second_invocation_blocked_when_locked`
- **Dimension 2.2** — DONE — with the lock free, the guarded entry acquires it and proceeds → Test `test_deploy_acquires_lock_when_free`

### §3 — Credential-rotation scripts carry the vault approval+auth gate

Both scripts read the vault without the approval friction or `op`-auth pre-check their `ip_allowlisting` siblings enforce. Source `common.sh` and call `playbooks_require_vault_read_approval` + `playbooks_require_op_auth` before the first read in each. A parity grep gate (in `check-playbooks`) then enforces the invariant across the whole operations surface — which also pulled the three ungated `observability/` scripts into scope (Discovery consult 4). **Implementation default:** keep the existing `op_read_with_retry` wrapper; the fix is additive (gates in front), not a rewrite of the retry/cache logic.

- **Dimension 3.1** — DONE — running either script with `ALLOW_VAULT_READS` unset exits non-zero with the approval error before any `op read` → Test `test_credential_rotation_blocks_without_approval`
- **Dimension 3.2** — DONE — with approval set but `op` unauthenticated, the script exits non-zero via `playbooks_require_op_auth` → Test `test_credential_rotation_requires_op_auth`
- **Dimension 3.3** — DONE — every script under `playbooks/operations/**` that contains an `op read` also sources `common.sh` and calls both gates → Test `test_ops_scripts_vault_gate_parity` (grep gate in `check-playbooks`)

### §4 — Architecture-doc gate validates every milestone identifier and actually runs

The gate hardcodes a frozen `M40..M51` alternation and is wired into no make target, so M52+ references are skipped and the whole script never executes automatically. Both halves must land or the gate stays decorative: validate every `M[0-9]+` reference against `done/`/`active/`, and add a `make check-architecture-doc` target to `lint-all`.

**Roadmap carve-out (Indy's Jul 10 call — see Discovery).** Unfreezing the range surfaces 39 identifiers across `docs/architecture`; 38 resolve to `done/`/`active/`. The lone survivor is `roadmap.md:49`, which cites `M105` (spec real, but in `pending/`) as a Rung-1 dependency — legitimate content for the one doc whose job is naming unshipped work. The strict `done/`+`active/` tier therefore binds every architecture doc that asserts a shipped fact; `roadmap.md` alone additionally resolves against `pending/`. A phantom identifier with no spec anywhere (`M999`) still fails in every file, `roadmap.md` included. This preserves the gate's original "pending specs are aspirational, not load-bearing" intent rather than dissolving it repo-wide.

- **Dimension 4.1** — a fixture architecture file citing an unshipped identifier (e.g. `M999`) makes the gate exit non-zero; the real corpus (which cites M52+ up to M121) resolves clean, proving M52+ is no longer skipped → Test `test_arch_doc_validates_all_m_ids`
- **Dimension 4.3** — a fixture `roadmap.md` citing a `pending/`-only identifier resolves clean, while the same citation in any other architecture file fails; `M999` fails in both → Test `test_arch_doc_roadmap_resolves_pending`
- **Dimension 4.2** — `check-architecture-doc` exists in `make/quality.mk` and is a prerequisite of `lint-all` → Test `test_arch_doc_wired_into_lint_all` (grep of the make file)

### §5 — Route-registration gate sees underscore-named targets

Both make-target regexes exclude underscore, so an underscore-named target cited in the REST guide is captured by neither the citation scan nor the definition set — a false negative if a phantom internal target is ever cited. Widen both char classes to include underscore.

**Correction to read-first note 3.** That note claims `MAKE_TARGET_DEF_RE`'s `_?` prefix "already anchors underscore-led names, so only the inner char class needs `_`." It does not: the `_?` sits *outside* the capture group, so `_fmt:` registers under the name `fmt`. The `_?` moves inside the group as part of this Section — a strict tightening (a doc citing `` `make fmt` `` now correctly reports `PHANTOM TARGET`; no target cited in the guide today changes verdict).

- **Dimension 5.1** — the widened `DOC_MAKE_TARGET_RE` captures `` `make _fmt_check` `` and `MAKE_TARGET_DEF_RE` matches a `_lint_zig_test_depth:` definition line → Test `test_underscore_targets_captured`
- **Dimension 5.2** — a doc citing `` `make _no_such_target` `` (a phantom underscore target) is reported as a `PHANTOM TARGET`, proving underscore citations are now checked → Test `test_phantom_underscore_target_flagged`

## Interfaces

```
deploy.sh is_already_installed() -> exit 0 (installed, skip) | non-zero (reinstall)
  matches iff the field-2 version token equals "${VERSION#v}" exactly.
deploy.sh main() -> holds flock -n on /var/lock/agentsfleet-deploy.lock for its
  whole run; exits non-zero without side effects if the lock is already held.
credential_rotation/{01_vault_sync,02_service_health}.sh -> exit non-zero before
  any vault read unless ALLOW_VAULT_READS=1 and `op` is authenticated.
check_architecture_doc.sh -> exit 0 iff every M[0-9]+ reference resolves against
  done/ or active/; roadmap.md additionally resolves against pending/. Run by
  `make check-architecture-doc` (in lint-all).
check_route_registration_doc.py -> make-target scans accept [a-z0-9_-] names.
```

No command-line surface, environment-variable name, on-disk path, or gate exit-code convention changes beyond the lock path and the new make targets above.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Version substring collision | pre-release/build-metadata suffix or two-part-tag collision | exact-equality check reports not-installed → full reinstall (Dimension 1.1) |
| Malformed `--version` output | binary shape drift or read error | resolves to not-installed → safe reinstall, never a wrong skip (Dimension 1.3) |
| Overlapping deploy | manual prod run or cancel-orphaned CI run | second invocation fails fast on the held lock (Dimension 2.1) |
| Vault read without approval | operator forgets `ALLOW_VAULT_READS=1` | script exits non-zero with the approval message before any read (Dimension 3.1) |
| `op` unauthenticated | expired session | `playbooks_require_op_auth` exits non-zero with the sign-in hint (Dimension 3.2) |
| Unresolvable milestone reference | doc cites a never-shipped identifier | gate exits non-zero naming the reference (Dimension 4.1) |
| Phantom underscore target cited | internal `_`-target named in the public guide | gate reports `PHANTOM TARGET` (Dimension 5.2) |

## Invariants

1. `deploy.sh` compares the exact version token, never a glob substring — enforced by Dimensions 1.1/1.2 and RULE ORP grep confirming no `== *"…"*` remains around `VERSION`.
2. At most one `deploy.sh main()` runs per host — enforced by `flock -n` on a fixed lock path (Dimension 2.1).
3. Every `playbooks/operations/**` script that reads the vault passes both gates — enforced by the `check-playbooks` parity grep (Dimension 3.3), not review.
4. `check_architecture_doc.sh` carries no frozen milestone range and runs in `lint-all` — enforced by the validate-all scan plus the make-membership grep (Dimensions 4.1/4.2). `pending/` resolves in `roadmap.md` and nowhere else (Dimension 4.3).
5. The route-registration checker's make-target regexes accept underscore — enforced by the self-test (Dimension 5.1).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | deploy Discord notifications and gate stderr keep their existing shape; no event added, renamed, or removed | unchanged | no secret material printed (§3 keeps refs/checkmarks only) | existing behavior; the new tests assert exit codes, not events |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_deploy_version_substring_not_equal_reinstalls` | stub `--version` → `0.1.0-rc1`, target `v0.1.0` → `is_already_installed` non-zero |
| 1.2 | unit | `test_deploy_version_exact_match_skips` | stub `--version` field 2 equals `${VERSION#v}` → `is_already_installed` zero |
| 1.3 | unit | `test_deploy_malformed_version_reinstalls` | stub `--version` → empty / no field 2 → `is_already_installed` non-zero (reinstall) |
| 2.1 | unit | `test_deploy_second_invocation_blocked_when_locked` | background holder owns the lock → guarded entry exits non-zero, no `install`/`systemctl` reached. `flock` absent → loud SKIP locally, hard fail when `CI` is set |
| 2.2 | unit | `test_deploy_acquires_lock_when_free` | free lock → guarded entry acquires it and continues. Same `flock` skip rule as 2.1 |
| 3.1 | unit | `test_credential_rotation_blocks_without_approval` | `ALLOW_VAULT_READS` unset → each script exits non-zero + approval message, zero `op read` invoked |
| 3.2 | unit | `test_credential_rotation_requires_op_auth` | approval set, `op whoami` stubbed to fail → exit non-zero + sign-in hint |
| 3.3 | integration | `test_ops_scripts_vault_gate_parity` | grep across `playbooks/operations/**`: every file with `op read` also sources `common.sh` and calls both gates → 0 offenders |
| 4.1 | unit | `test_arch_doc_validates_all_m_ids` | fixture arch dir citing `M999` (no spec) → gate exit non-zero; real corpus → exit 0 |
| 4.2 | unit (grep) | `test_arch_doc_wired_into_lint_all` | `make/quality.mk` defines `check-architecture-doc` and lists it in `lint-all` |
| 4.3 | unit | `test_arch_doc_roadmap_resolves_pending` | fixture `roadmap.md` citing a `pending/`-only ID → exit 0; same ID in `direction.md` → non-zero; `M999` in `roadmap.md` → non-zero |
| 5.1 | unit | `test_underscore_targets_captured` | `DOC_MAKE_TARGET_RE` finds `_fmt_check`; `MAKE_TARGET_DEF_RE` matches `_lint_zig_test_depth:` |
| 5.2 | unit | `test_phantom_underscore_target_flagged` | doc text citing `make _no_such_target` → checker returns a `PHANTOM TARGET` violation |
| all shell | integration (regression) | `make lint-shell` | shellcheck stays green on every edited/new `*.sh` (`--severity=error`) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Deploy version + lock tests pass (§1/§2) | `bash deploy/baremetal/deploy_test.sh` | exit 0 | P0 | |
| R2 | Rotation scripts block without approval (§3) | `bash playbooks/operations/credential_rotation/vault_gate_test.sh` | exit 0 | P0 | |
| R3 | No raw-vault-read parity gap remains (§3) | `make check-playbooks` | exit 0 (parity grep clean) | P0 | |
| R4 | Frozen milestone range gone (§4) | `grep -n 'M(40\|41\|42' scripts/check_architecture_doc.sh` | no output | P0 | |
| R5 | Architecture gate runs in lint-all (§4) | `grep -n 'check-architecture-doc' make/quality.mk` | ≥2 matches (target + lint-all) | P0 | |
| R6 | Route-reg underscore self-test passes (§5) | `python3 scripts/check_route_registration_doc_test.py` | exit 0 | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| R8 | Unfrozen arch gate green on the real corpus (§4) | `bash scripts/check_architecture_doc.sh` | exit 0 | P0 | |
| R9 | Arch-gate self-test passes (§4, incl. roadmap carve-out) | `bash scripts/check_architecture_doc_test.sh` | exit 0 | P0 | |
| S1 | Shell lint clean | `make lint-shell` | exit 0 | P0 | |
| S2 | Full lint clean (incl. new gates) | `make lint-all` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| frozen milestone alternation | `grep -rn 'M(40\|41\|42\|43' scripts/` | 0 matches |
| substring version glob | `grep -n '== \*"\${VERSION' deploy/baremetal/deploy.sh` | 0 matches |

## Out of Scope

- De-duplicating the identical `op_read_with_retry` wrapper across the two `credential_rotation` scripts into `common.sh` — a possible follow-up cleanup; this spec keeps the fix additive.
- Any change to `src/runner/cmd/version.zig`'s output shape — §1 parses the existing format; it is not modified.
- A blocking/queueing deploy lock or a full retry/backoff policy — §2 delivers fail-fast mutual exclusion only.
- Broadening the vault-approval gate to `playbooks/founding/**` or other surfaces — this spec covers the `credential_rotation` parity gap and the operations-wide grep only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reruns a deploy and it either truly skips (exact version already live) or truly reinstalls, never silently leaves a stale binary; a second concurrent deploy fails fast with "already running"; a rotation script refuses to touch the vault until the operator sets approval and signs in, matching every other operations script.
2. **Preserved user behaviour** — the deploy happy path (correct install + restart + Discord notify), the rotation scripts' actual checks, and both gates' passing behavior on clean input are unchanged; only the failure/edge branches tighten.
3. **Optimal-way check** — each fix is the most direct one: an exact-equality parse, a `flock -n` wrap, a source-and-call preamble mirroring a sibling, a validate-all scan plus a make wire, a char-class widening. No larger refactor is warranted.
4. **Rebuild-vs-iterate** — iterate: five contained edits to existing scripts, each with a working sibling or precedent to converge onto; nothing wants a redesign.
5. **What we build** — an exact version parse, a deploy lock, two vault-gate preambles + a parity grep, a validate-all architecture scan wired into a make target, and a two-line regex widening + a live regression citation.
6. **What we do NOT build** — a queueing lock, a retry/backoff policy, a `common.sh` wrapper de-dup, or any change to `version.zig` — see Out of Scope.
7. **Fit with existing features** — compounds the existing `make lint-all` gate suite and the `playbooks/lib/common.sh` guardrail convention; must not destabilize the deploy happy path or the sibling `ip_allowlisting` behavior it mirrors.
8. **Surface order** — N/A — no user surface. These are operator deploy scripts and repo Continuous Integration (CI) gates; there is no end-user product surface.
9. **Dashboard restraint** — N/A — no user surface. No UI, controls, or quality claims are added.
10. **Confused-user next step** — N/A — no user surface. The operator-facing recovery is the gate's own stderr (the approval message names `ALLOW_VAULT_READS`; the lock message names the held path; the deploy usage text is unchanged).

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections, one per independent finding — two on `deploy.sh` (distinct concerns: version equality vs run serialization), one on the `credential_rotation` parity gap, one on the architecture gate (repair + wire, both halves), one on the route-registration regex. Each is independently testable and DONE-markable.
- **Alternatives considered:** (a) collapsing §1+§2 into one `deploy.sh` Section — rejected: version equality and lock serialization are separately testable concerns and separately revertible; (b) banning raw `op read` repo-wide and routing every read through a new retry-capable `common.sh` helper — rejected as scope creep; the additive gate preamble + parity grep closes the finding without a wrapper refactor.
- **Patch-vs-refactor verdict:** this is a **patch** across existing scripts; the only structural addition (a `flock` wrapper and a new make target) hardens rather than restructures. The larger `common.sh` retry-wrapper de-dup is named in Out of Scope rather than mud-patched in here.

## Discovery (consult log)

- **Consults** — three judgment calls surfaced at PLAN, before any edit; Indy decided all three on Jul 10, 2026.

  1. **§4 — the unfrozen gate fails the real corpus.** Removing the `M(40..51)` alternation surfaces 39 identifiers in `docs/architecture`; `roadmap.md:49` cites `M105`, whose spec is real but sits in `pending/`, which the gate's own comment rejects as "aspirational, not load-bearing". So Dimension 4.1's claim that "the real corpus resolves clean" was false as written. Options put to Indy: (a) carve out `roadmap.md`, (b) accept `pending/` repo-wide, (c) reword `roadmap.md`. **Indy chose (a)** — strict `done/`+`active/` for every doc asserting a shipped fact; `roadmap.md` alone also resolves `pending/`, because naming unshipped work is that file's purpose. `M999` still fails everywhere. §4, Interfaces, Invariant 4, and Dimension 4.3 updated to match.

  2. **§2 — `flock` does not exist on macOS.** `deploy.sh` targets Linux bare metal, and `flock` is the correct primitive there (the kernel releases the lock when a `SIGKILL`ed deploy dies; a `noclobber` lock file would strand every later deploy behind stale state). The exposure is the *test*: `make lint-all` runs on Indy's mac. Options: (a) hard-fail with a `brew install flock` hint, (b) skip locally + hard-fail when `CI` is set, (c) drop `flock` for a portable `noclobber` lock. **Indy chose (b)** — no new local tool prereq; the lock Dimensions are enforced on every `ubuntu-latest` runner and can never be silently green in CI. Recorded in §2 and the Test Specification.

  3. **`src/runner/cmd/version.zig` documents the deleted behavior.** Its module doc comment justifies the output shape by citing `is_already_installed()`'s substring test (`current == *"${VERSION#v}"*`), and a test comment repeats it. §1 deletes that behavior, so both comments become false. The file was outside Files Changed, and Out of Scope forbids changing its *output shape* — which a comment-only edit does not. **Indy approved the comment-only fix**; the file is now in Files Changed so acceptance row R7 stays green.

  4. **§3 — the operations-wide parity grep fails on three more scripts.** Dimension 3.3 demands a grep proving every `playbooks/operations/**` script that reads the vault calls both gates. Three do not: `observability/01_credentials.sh`, `02_prometheus.sh`, `03_dashboard.sh` read six 1Password references with no approval prompt, no `op`-auth pre-check, and no `common.sh` source. Options put to Indy: (a) gate all three here, (b) narrow the grep to `credential_rotation/` + `ip_allowlisting/`, (c) keep the wide grep with a named `observability/` exclusion plus a follow-up spec. **Indy chose (a)** — (b) makes Invariant 3 false while looking green, and (c) is the deferred-cleanup carve-out RULE NLG bans pre-`2.0.0`. The three scripts join Files Changed. Consequence: `observability/00_gate.sh` now requires `ALLOW_VAULT_READS=1` and an authenticated `op`, matching every sibling.

- **Gate-flag triage** — one mechanical finding, auto-applied. The spec's read-first note 3 mis-describes the §5 defect: `MAKE_TARGET_DEF_RE`'s `_?` sits *outside* the capture group, so `_fmt:` registers under the name `fmt` (not `_fmt`). Widening the inner char class alone would leave that wrong. The `_?` moves inside the group. Strict tightening: a doc citing `` `make fmt` `` now correctly reports `PHANTOM TARGET`; no target cited in the guide today changes verdict. Recorded as a correction note under §5.

- **Architecture consult** — `docs/architecture/direction.md` (platform determinism + gate discipline) read before §4. The direction doc's stance — a gate that cannot fail is not a gate — is what forces both halves of §4 (unfreeze *and* wire into `lint-all`) to land together, and is why option (b) was chosen in consult 2 over a permanently-skipped local test.

- **Metrics review** — not applicable; no product or operator signal changes (see Metrics & Observability).
- **Skill-chain outcomes** — recorded at VERIFY / CHORE(close).
- **Deferrals** — none.
