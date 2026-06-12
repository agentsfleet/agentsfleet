<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_003: Rename binaries, make targets, and infra strings to the agentsfleet set (`agentsfleet`, `agentsfleetd`, `agentsfleet-runner`)

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 003
**Date:** Jun 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the operator-facing brand seam: every install, `make` invocation, container pull, and systemd unit still says zombie after M92_002 flipped the identity surfaces
**Categories:** API, CLI, INFRA, OBS
**Batch:** B3 — independent of M92_001 (B2); follows M92_002 (B1, merged #405; wordmark continuation #406 in flight, non-blocking)
**Branch:** feat/m92-003-agentsfleet-binaries
**Test Baseline:** unit=1946 integration=189
**Depends on:** M92_002 (the rename principle, eval patterns, and brand assets this spec extends)
**Provenance:** agent-generated (rename session with Indy, Jun 12, 2026 — name mapping Indy-decided in-session: "i think zombied is agentsfleetd"); blast radius measured on main b0e843ff; re-confirm at PLAN.

**Canonical architecture:** no component-shape change — naming pass only. `docs/architecture/*.md` stays authoritative for flows; M92_002 §3 deliberately kept operational binary names in those docs, and this spec is the cutover that flips them (architecture-consult per `dispatch/name_architecture.md` before each flow-name edit).

---

## Implementing agent — read these first

1. `docs/v2/active/M92_002_P1_DOCS_UI_AGENTSFLEET_REBRAND_IDENTITY.md` — the rename principle (brand strings flip; strings that resolve keep resolving) and the E7 HEAD-vs-tree count-compare eval pattern this spec inherits for its keep-ledger.
2. `build.zig` — the daemon artifact name and how zig-out binary names propagate into `make/build.mk`, the workflows, and the Dockerfile. The runner is NOT built here (its own build graph — locate via the §1 ledger).
3. `Makefile` + `make/*.mk` + `.githooks/pre-commit` — target naming and the hook lanes that launch targets by name; hook path-globs stay on the unchanged directories (`agentsfleet/*`), only the launched target names flip.
4. `deploy/fly/zombied-dev/fly.toml` — the keep/flip seam inside one file: `app = "zombied-dev"` keeps (fly resolves it), `image = "ghcr.io/usezombie/zombied:dev-latest"` flips.
5. `ui/agentsfleet.dev/dist/install.sh` + `ui/agentsfleet.dev/install_test.sh` — the installer whose installed-binary name flips while the `usezombie.sh` domain stays.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `chore(m92): rename binaries + targets zombie* -> agentsfleet*`
- **Intent (one sentence):** every binary, make target, container image, compose service, deploy file, and dashboard reference presents the agentsfleet names — `agentsfleet` (Command-Line Interface, CLI), `agentsfleetd` (daemon), `agentsfleet-runner` — while every identifier an external system still resolves (fly app names, npm package name, schema, org URLs, directories) keeps working untouched.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the live world: (a) fresh blast-radius grep matches the §1 ledger, (b) Indy's image-push step is agreed and sequenced (build from this branch, push, paste manifest-verify), (c) the baremetal host running `zombie-runner.service` is enumerated for the §3 checklist. A `[?]` blocks EXECUTE.

**The rename principle (inherited, load-bearing):** *names operators type and see get renamed; identifiers external systems resolve keep resolving until their own cutover.* Flips now: binary/artifact names, make targets, hook-launched target names, workflow strings, GitHub Container Registry (ghcr) image names under the unchanged org, compose service/container names, fly `image` lines, deploy-file contents, grafana references, installer-installed binary, architecture-doc operational names, and (Indy-amended, see Discovery) npm package names — `@agentsfleet/design-system` → `@agentsfleet/design-system`, `agentsfleet-app`/`agentsfleet-website` → `agentsfleet-app`/`agentsfleet-website` (all private, repo-local), `@usezombie/zombiectl` → `@agentsfleet/cli` (public — registry cutover gated on §4's verified first publish). Untouched: fly `app` values (`zombied-dev`/`zombied-prod`) and cloudflared hostname refs, directory paths (`agentsfleet/`, `src/agentsfleetd/`), `usezombie-admin` Postgres user/db, `core.zombie_*` schema, `x-usezombie*` headers, `github.com/usezombie`, `team@usezombie.com`. The installer domain flips `usezombie.sh` → `agentsfleet.dev` (Indy-amended) — gated on the new host serving the installer; the old domain keeps serving/aliasing so existing snippets never dead-end.

---

## Product Clarity

1. **Successful user moment** — an operator runs the install one-liner and the tool that lands on `PATH` is `agentsfleet`; `agentsfleet --help` answers; on the server `ps` shows `agentsfleetd`; `docker pull ghcr.io/usezombie/agentsfleetd:dev-latest` resolves. The zombie names are out of the operator's hands.
2. **Preserved user behaviour** — the install one-liner keeps installing throughout the cutover (`usezombie.sh` serves until `agentsfleet.dev` is verified, then aliases to it); the fly apps keep answering on their current hosts; API endpoints and headers unchanged; Continuous Integration (CI) lanes run the same checks under renamed targets; existing ghcr images stay pullable (new names land alongside, old tags untouched).
3. **Optimal-way check** — a token rename with an eval-pinned keep-ledger is the whole job. The unconstrained optimal (also rename org, npm scope, fly apps, directories in one sweep) is rejected: each keep has an external resolver (GitHub, npm registry, fly platform) deserving its own enumerated cutover.
4. **Rebuild-vs-iterate** — iterate; zero behaviour change, pure naming. Determinism preserved by pinning both directions (flips complete, keeps byte-stable) with count-compare evals.
5. **What we build** — the name map across `build.zig`, the runner build graph, npm `bin`, make targets, hook launch lines, six workflows, `Dockerfile`, compose, fly `image` lines, `deploy/baremetal/*`, grafana, the installer, and architecture-doc operational names — plus the keep-ledger evals and Indy's two external checklist rows (ghcr push, baremetal unit migration).
6. **What we do NOT build** — org rename; fly app + cloudflared hostname cutover; schema/user renames; mail cutover; `api.usezombie.com`; deprecation shims (the old npm listing gets a deprecation pointer later, nothing more); M92_001 copy. One-line reasons in Out of Scope.
7. **Fit with existing features** — completes the seam M92_002 opened (identity then, operational names now); must not destabilize the install path (`install_test.sh` is the guard) or dev deploys (image refs flip only after Indy's push is manifest-verified).
8. **Surface order** — CLI-first by nature: the CLI binary name is the headline; UI untouched.
9. **Dashboard restraint** — N/A; grafana only re-points existing names, no new panels or claims.
10. **Confused-user next step** — typing `agentsfleet` post-upgrade: command-not-found; the installer's completion message and README name `agentsfleet`. Hard cutover, no shim — pre-launch blast radius, Indy-ratified.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC (no alias binaries or dead targets left behind), RULE NLR (touched make/workflow files shed stale zombie comments), RULE NLG (no legacy shims pre-2.0.0 — the hard-cutover decision), RULE ORP (cross-layer orphan sweep on every renamed target, binary, unit file, dist entry), RULE TST-NAM.
- **`dispatch/write_zig.md`** — the `build.zig` edit (ZIG GATE; cross-compile both linux targets).
- **`dispatch/write_ts_adhere_bun.md`** — `agentsfleet/package.json` + build-script edits.
- **`dispatch/name_architecture.md`** — binary names appear in architecture flows; consult before each flip (no override).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `build.zig` artifact rename | one-line `.name` edits; cross-compile `x86_64-linux` + `aarch64-linux` |
| PUB / Struct-Shape | no — no new pub surface | — |
| File & Function Length | no — renames; no file grows | — |
| UFS (literals) | yes — image/binary names recur across infra files that cannot share constants | counts pinned by the keep/flip evals instead of constants; no new literals in `*.ts`/`*.zig` beyond existing constant sites |
| UI Substitution / DESIGN TOKEN | no — no UI markup | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no runtime logic or schema surface | — |
| CI/CD edit guard (`.github/workflows/**`) | yes — six workflows | Indy-granted in the Jun 12, 2026 session (scope: binary/target/image name strings only, zero workflow-logic changes); each workflow listed in the PR body |

---

## Overview

**Goal (testable):** after merge and the verified image push, `zig build` emits `agentsfleetd` (and the runner graph `agentsfleet-runner`) with no zombie-named binary in any build output, the npm package installs an `agentsfleet` bin, every renamed make target resolves and the full suite is green in CI, `docker manifest inspect ghcr.io/usezombie/agentsfleetd:dev-latest` succeeds, and a repo-wide word-boundary grep for `agentsfleet|zombied|zombie-runner` matches only the eval-pinned keep ledger.

**Problem:** M92_002 rebranded what users read; every name operators *type and run* — the install target, the daemon process, the runner unit, the image pulls, the make targets — still says zombie.

**Solution summary:** a keep-ledgered rename pass: re-enumerate the blast radius, flip the operator-facing name set in one branch, pin every deliberate non-flip with HEAD-vs-tree count compares, gate image-dependent references on Indy's manual ghcr push, and hand the baremetal unit migration as a verified checklist row.

---

## Prior-Art / Reference Implementations

- **Rename-pass pattern** → M92_002: enumerate → verify → flip → eval-pin both directions; its E7 count-compare is reused verbatim for this spec's keep ledger.
- **CLI** → the 7 Pillars apply unchanged: this spec renames the binary only; command → handler → errors structure, output-as-a-service, and the 3-tier test pyramid are untouched (divergence: none — no handler edits permitted).
- **Deploy seam** → `deploy/fly/zombied-dev/fly.toml` is its own prior art: the file already separates platform identity (`app`) from artifact identity (`image`).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig` | EDIT | daemon artifact name → `agentsfleetd` |
| `src/zombied/` → `src/agentsfleetd/` (directory) | RENAME | `audits/error-codes.sh` pre-points at the new path (Indy); do-it-all decision; zon fingerprint refreshed |
| runner build entry (located by §1 ledger) | EDIT | runner output name → `agentsfleet-runner` |
| `Makefile`, `make/{acceptance,build,quality,test-unit,test-integration}.mk` | EDIT | token-bearing targets renamed; binary path refs follow |
| `.githooks/pre-commit`, `.githooks/pre-push` | EDIT | launched target names flip; path-globs stay (directories unchanged) |
| `.github/workflows/{deploy-dev,lint,post-release,release,test,cross-compile}.yml` | EDIT | strings-only: target, binary, image names |
| `Dockerfile` | EDIT | built/copied binary name; image labels |
| `docker-compose.yml` | EDIT | `zombied-api`→`agentsfleetd-api`, `zombie-postgres`→`agentsfleet-postgres`, `zombie-redis`→`agentsfleet-redis` |
| `deploy/fly/zombied-{dev,prod}/fly.toml` | EDIT | `image` lines flip; `app` lines byte-stable |
| `deploy/baremetal/zombie-runner.service` → `agentsfleet-runner.service` | RENAME | unit name + ExecStart binary |
| `deploy/baremetal/deploy.sh` | EDIT | unit/binary refs + old-unit→new-unit transition handling |
| `deploy/grafana/runner_fleet.json` | EDIT | job/binary name refs |
| `agentsfleet/package.json` | EDIT | name → `@agentsfleet/cli`, `bin` key → `agentsfleet`, entry → `./dist/bin/agentsfleet.js` |
| `agentsfleet/` build config + tests referencing `dist/bin/agentsfleet.js` | EDIT | emit + spawn the renamed entry |
| `ui/packages/design-system/package.json` + every importing file | EDIT | scope flip → `@agentsfleet/design-system` (private; imports, lockfile, workspace refs) |
| `ui/packages/{app,website}/package.json` + workspace refs | EDIT | private names → `agentsfleet-app` / `agentsfleet-website` |
| `ui/agentsfleet.dev/dist/install.sh`, `ui/agentsfleet.dev/install_test.sh` | EDIT | `PKG` → `@agentsfleet/cli` (publish-gated); printed binary + self-referencing domain examples (domain-gated) |
| `ui/packages/website/src/config.ts` + `config.test.ts` + `README.md` install snippet | EDIT (domain-gated) | `INSTALL_COMMAND` → the `agentsfleet.dev` one-liner once Eval `E10` passes; pins updated in the same conscious edit |
| `docs/architecture/*.md` (operational-name bearing, per §1 ledger) | EDIT | binary/flow names follow; schema/host/user names stay |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream — the renames cross-reference (targets ↔ workflows ↔ hooks ↔ binaries); a partial flip leaves CI red or an operator typing a dead name. Sections slice by surface, not by token.
- **Alternatives considered:** per-component workstreams (rejected: every intermediate state breaks a caller); folding into the eventual org-rename spec (rejected: org/npm/fly-app each wait on external resolvers; binaries don't).
- **Patch-vs-refactor verdict:** **patch** (naming pass with verification gates). Follow-up named: the org/npm/fly-app/mail cutover spec (M9X).

---

## Sections (implementation slices)

### §1 — Blast-radius ledger & keep-pins (blocks every flip)

Fresh repo-root `git grep -rn -w` per token (`agentsfleet`, `zombied`, `zombie-runner`), no path filter; every hit lands in a flip-or-keep ledger (production and test files separated) appended to Discovery. Architecture-consult fires for flow names. The keep-pin eval baseline is captured before any edit.

- **Dimension 1.1** — DONE — ledger complete (class table in Discovery); every hit dispositioned flip/keep → Eval `E1` definition matches the ledger
- **Dimension 1.2** — DONE — keep-pin baseline recorded (Discovery: counts pre-flip) → Eval `E3`

### §2 — Daemon: `zombied` → `agentsfleetd`

`build.zig` artifact, zig-out consumers (make targets, `Dockerfile`, workflows), hook lane prefixes.

- **Dimension 2.1** — DONE — `zig build` emits `agentsfleetd` only; verified in-worktree → Eval `E2`
- **Dimension 2.2** — cross-compile both linux targets green → Acceptance row

### §3 — Runner: `zombie-runner` → `agentsfleet-runner`

Runner build output, `deploy/baremetal/` unit rename + `deploy.sh` transition (stop/disable old unit, install/enable new), and the host-migration checklist handed to Indy.

- **Dimension 3.1** — DONE — `build_runner.zig` emits `agentsfleet-runner`; unit file renamed `agentsfleet-runner.service`; deploy.sh transition follows → Eval `E1`, negative test on old unit name
- **Dimension 3.2** — host checklist surfaced and verify output captured (e2e, manual-verified — M92_002 Dimension 1.1 pattern) → Discovery entry

### §4 — CLI, packages, installer (three independent gates)

Repo-local now: `bin` key, dist entry, every caller, workspace package renames. Behind their own gates: the public npm package (first publish) and the installer domain (host serves). A parked gate parks only its surface — M92_002's 6.1 pattern.

- **Dimension 4.1** — manifest pair flipped (name `@agentsfleet/cli`, `bin` `agentsfleet` → renamed entry) → Test `test_cli_bin_name_agentsfleet` + Eval `E3`
- **Dimension 4.2** — CLI acceptance suite green spawning the renamed entry → existing suite under the renamed target
- **Dimension 4.3** — installer installs `agentsfleet`; `install_test.sh` green → Eval `E7`
- **Dimension 4.4** — workspace packages renamed (`@agentsfleet/design-system`, `agentsfleet-app`, `agentsfleet-website`); imports/lockfile/workspace refs follow; app + website suites green → Eval `E1`
- **Dimension 4.5** — registry cutover: `@agentsfleet` npm org exists (Indy), publish token covers it, `release.yml` publishes the new name, first publish verified; installer `PKG` flips only after Eval `E9` passes (e2e, manual-verified)
- **Dimension 4.6** — installer-domain cutover: `agentsfleet.dev` serves the installer (Indy: registrar + DNS + hosting attach + `usezombie.sh` alias); `INSTALL_COMMAND`, its pins, and README snippet flip in the same gated edit; unverified → parks → Eval `E10`

### §5 — Make targets + hooks

Every token-bearing target renamed; every caller updated (workflows ride §6, hooks here). Hook path-globs stay on directories.

- **Dimension 5.1** — DONE — Eval `E4` empty; renamed lanes ran green in-worktree (`test-unit-agentsfleetd`, `test-unit-zigrunner`)
- **Dimension 5.2** — staging a `agentsfleet/`-path file fires the renamed lint lane → hook-fire check recorded in Discovery

### §6 — Workflow strings pass

Six workflows, strings-only; the PR body lists each. Full CI on the PR is the functional proof the renamed targets/binaries wire up.

- **Dimension 6.1** — DONE (eval) — Eval `E5` empty (strings-only); full-pipeline proof = the PR's CI run, linked in Verification Evidence

### §7 — Containers, compose, fly-image seam

`Dockerfile`, compose names, fly `image` lines. Indy builds from this branch and pushes `ghcr.io/usezombie/agentsfleetd` + `ghcr.io/usezombie/agentsfleet-runner`; merge waits on manifest verification so deploys never pull a missing image.

- **Dimension 7.1** — DONE — compose/fly/Dockerfile flipped; `app =` lines byte-stable (E3 baseline + spec-prose delta only) → Eval `E3` + `E1`
- **Dimension 7.2** — new-name images pushed and verified (e2e, manual-verified: Indy push, agent `docker manifest inspect`) → Eval `E6` output in PR body

### §8 — Observability + architecture docs

Grafana job/binary refs; `docs/architecture/` operational names flip (the set M92_002 §3 deliberately kept), schema/user/host names stay.

- **Dimension 8.1** — DONE — grafana dashboards re-pointed (incl. uid `agentsfleet-runner-fleet`; provisioning re-imports, old uid noted in PR body) → Eval `E1`
- **Dimension 8.2** — DONE — architecture + operational docs flipped (archive untouched) → Eval `E1`

---

## Interfaces

Locked surface — changes here require amending this spec: fly `app` values; API endpoints and `x-usezombie*` headers; `config.ts` constant names. `INSTALL_COMMAND` flips exactly once, only after Eval `E10` verifies, with its `config.test.ts` pin updated in the same edit. The new names — `agentsfleet`, `agentsfleetd`, `agentsfleet-runner`, `ghcr.io/usezombie/{agentsfleetd,agentsfleet-runner}`, compose `agentsfleetd-api`/`agentsfleet-postgres`/`agentsfleet-redis`, `agentsfleet-runner.service`, `@agentsfleet/design-system`, `agentsfleet-app`, `agentsfleet-website`, `@agentsfleet/cli`, install host `agentsfleet.dev` — each change exactly once, in this spec.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| CI calls a stale target | a workflow caller missed the rename | Eval `E5` + full pipeline on the PR; a red lane blocks merge |
| Deploy pulls a missing image | merge lands before the image push | Acceptance gates merge on Eval `E6` manifest output in the PR body |
| fly app accidentally renamed | seam confusion inside fly.toml | Eval `E3` pins `app =` lines byte-identical; negative check per file |
| Operator types the old binary | hard cutover, no shim | command-not-found; installer completion text + README name `agentsfleet`; Indy-ratified pre-launch |
| Baremetal runner down post-rollout | host still on the old unit | `deploy.sh` transition (stop old, enable new) + §3 checklist verify before rollout completes |
| Hook lane stops firing | glob/target drift in `.githooks` | Dimension 5.2 hook-fire check; globs pinned to unchanged directories |
| Installer targets an unpublished package | `PKG` flipped before first publish | §4 gate: Eval `E9` must pass first; `install_test.sh` red blocks merge |
| Install snippets point at a dead domain | command flipped before `agentsfleet.dev` serves | Eval `E10` gates the flip; until then every surface keeps `usezombie.sh` (parked, surfaced) |

---

## Invariants

1. The install one-liner never points at a host that doesn't serve the installer — `config.test.ts` pins the current command until Eval `E10` passes; the flip updates command + pin in one edit.
2. The installer never npm-installs a package that doesn't resolve — `PKG` flips only after Eval `E9` succeeds; `install_test.sh` enforces end-to-end.
3. fly `app =` values byte-stable — Eval `E3`.
4. No build output emits a zombie-named binary — Eval `E2` negative assertion.
5. `core.zombie_*`, `usezombie-admin`, `x-usezombie*`, `github.com/usezombie`, `team@usezombie.com` appear in the diff zero times — Eval `E3` count compare across all keep tokens.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1–1.2 | eval | Evals `E1`, `E3` baseline | ledger matches grep reality; keep counts recorded pre-flip |
| 2.1 | eval | Eval `E2` | `zig build` output contains `agentsfleetd`, contains no `zombied` |
| 2.2 | integration | cross-compile both linux targets | both `zig build -Dtarget=…` invocations exit 0 |
| 3.1 | unit | negative grep on old unit name in `deploy/` | `zombie-runner.service` absent; new unit's ExecStart names `agentsfleet-runner` |
| 3.2 | e2e (manual-verified) | host checklist | old unit stopped/disabled, new unit active — verify output in Discovery |
| 4.1 | unit | `test_cli_bin_name_agentsfleet` | package manifest: name `@agentsfleet/cli`, bin key `agentsfleet` → `./dist/bin/agentsfleet.js` |
| 4.2 | e2e | existing CLI acceptance suite via the renamed target | subprocess spawns the renamed entry; suite green |
| 4.3 | e2e | Eval `E7` (`install_test.sh`) | installer lands `agentsfleet` on `PATH`; old name not installed |
| 4.4 | eval | Eval `E1` + app/website suites | zero stale workspace-package refs; suites green under the new names |
| 4.5 | e2e (manual-verified) | Eval `E9` | the published package resolves before `PKG` flips |
| 4.6 | e2e (manual-verified) | Eval `E10` | `agentsfleet.dev` serves shellscript content before command/pins flip |
| 5.1 | eval | Eval `E4` | zero token-bearing make targets; renamed targets resolve |
| 5.2 | e2e (manual-verified) | hook-fire check | staged `agentsfleet/`-path file launches the renamed lint lane |
| 6.1 | eval | Eval `E5` + CI link | workflow diffs strings-only; full pipeline green |
| 7.1 | eval | Evals `E1`, `E3` | compose/fly/Dockerfile flips in; `app =` byte-stable |
| 7.2 | e2e (manual-verified) | Eval `E6` | both new-name images manifest-inspectable on ghcr |
| 8.1–8.2 | eval | Eval `E1` | grafana + architecture docs clean vs keep allowlist |

**Regression:** `make test`, `make test-integration`, website suite, dry lane — all green under renamed targets (behaviour byte-identical, only names moved). **Idempotency/replay:** N/A — rename pass.

---

## Acceptance Criteria

- [ ] Ledger + keep baseline committed — verify: Evals `E1`, `E3` output in PR body
- [ ] Full suite green under renamed targets — verify: `make lint && make test && make test-integration`
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] Installer green — verify: Eval `E7`
- [ ] Workflows strings-only + pipeline green — verify: Eval `E5` + CI run link
- [ ] New-name images pushed + verified before merge — verify: Eval `E6` output in PR body
- [ ] `@agentsfleet` npm org + first publish verified, or `PKG` flip parked-with-surface — verify: Eval `E9` output in PR body
- [ ] Installer-domain flip verified or parked-with-surface — verify: Eval `E10` output in PR body
- [ ] Baremetal checklist surfaced; host migration verify captured — verify: Discovery entry (§3)
- [ ] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: Flip completeness — word-boundary grep matches ONLY the keep ledger (expect empty)
git grep -rnwE "agentsfleet|zombied|zombie-runner" -- ':!docs/v1' ':!docs/v2/done' ':!docs/greptile-learnings' \
  | grep -vE "agentsfleet/|src/agentsfleetd|app = \"zombied-(dev|prod)\"|cloudflared|core\.zombie|usezombie-admin|x-usezombie" | head
# E2: Build artifacts — new daemon name present, old absent
zig build && ls zig-out/bin | grep -q agentsfleetd && ! ls zig-out/bin | grep -qx zombied && echo PASS
# E3: Keep-pins — HEAD-vs-tree count compare per keep token (expect all OK)
for t in "app = \"zombied-" "core\.zombie_" "usezombie-admin" "x-usezombie" "github\.com/usezombie"; do a=$(git grep -c "$t" origin/main | awk -F: '{s+=$NF}END{print s+0}'); b=$(grep -rc "$t" --exclude-dir=node_modules --exclude-dir=.git . | awk -F: '{s+=$NF}END{print s+0}'); echo "$t $([ "$a" = "$b" ] && echo OK || echo DRIFT)"; done
# E9: Published package resolves (run after Indy's first publish; gates the PKG flip)
npm view @agentsfleet/cli dist-tags --json && echo PASS
# E10: Installer domain serves (gates the INSTALL_COMMAND flip; until PASS that surface parks)
curl -fsSI https://agentsfleet.dev | grep -i "text/x-shellscript" && echo PASS
# E4: Make targets — no token-bearing target remains (expect empty)
make -qp 2>/dev/null | awk -F: '/^[A-Za-z0-9][^=\t]*:([^=]|$)/{print $1}' | grep -E "agentsfleet|zombied|zombie-runner" | head
# E5: Workflow diffs strings-only (expect empty)
git diff origin/main -- .github/ | grep -E "^[-+]" | grep -vE "^[-+]{3}|agentsfleet|zombied|zombie-runner|agentsfleet" | head
# E6: Images pushed (Indy step; run after push)
for i in agentsfleetd agentsfleet-runner; do docker manifest inspect "ghcr.io/usezombie/$i:dev-latest" >/dev/null && echo "$i OK"; done
# E7: Installer — lands the new binary name
(cd ui/agentsfleet.dev && ./install_test.sh)
# E8: Gitleaks — gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `deploy/baremetal/zombie-runner.service` | `test ! -f deploy/baremetal/zombie-runner.service` |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| emitted `dist/bin/agentsfleet.js` | build output listing post-`bun run build` | absent |
| `zombie-runner.service` refs | Eval `E1` | 0 matches outside keep ledger |
| old make target names | Eval `E4` | 0 matches |

---

## Discovery (consult log)

- **Authoring-time decisions (Indy, Jun 12, 2026 evening session):** name map decided — CLI bare `agentsfleet`; daemon `agentsfleetd` (> Indy: "i think zombied is agentsfleetd"); runner `agentsfleet-runner`; compose locals `agentsfleet-postgres`/`agentsfleet-redis`. Hard cutover, no alias binaries (pre-2.0.0, RULE NLG). Directories stay. `.github/workflows` edits granted, strings-only scope. Container sequencing: Indy builds from this branch and pushes the new-name images manually ("I will build and push new containers now?" → sequenced post-branch, pre-merge, manifest-verified). Operating-model target-name prose in dotfiles rides a companion dotfiles commit at cutover.
- **Scope amendment (Indy, Jun 12, 2026 evening, mid-session):** packages + installer domain fold in — "> Indy: \"@agentsfleet/design-system is @agentsfleet/design-system\"", "> Indy: \"@usezombie/zombiectl is @agentsfleet/cli\"", "> Indy: \"usezombie.sh is agentsfleet.dev\"". Verified at amendment: design-system/app/website are `private: true` (repo-local flips); `@usezombie/zombiectl` is public (`release.yml` `npm publish` + installer `PKG=`) → Indy rows: create the `@agentsfleet` npm org, publish token coverage, first publish — `PKG` flip gated on Eval `E9`; `agentsfleet.dev` publishes no DNS records while `usezombie.sh` serves the installer (200, `text/x-shellscript`) → Indy rows: registrar/DNS/hosting attach + old-domain alias — command/pin/README flips gated on Eval `E10`. Dotfiles governance refs (`dispatch/write_ts_adhere_bun.md` scope examples, `AGENTS.md` worktree command, `api-dev.usezombie.com` in verify docs, docs-URL examples) ride the companion dotfiles commit at merge + their own host cutovers.
- **Mid-EXECUTE decisions (Indy, Jun 13, 2026):** UFS string-dup on `service_report.zig` → single-binding fix (Indy-approved). `audits/error-codes.sh` found pre-pointed at `src/agentsfleetd/errors/` (Indy forward-edit) → directory rename `src/zombied/` → `src/agentsfleetd/` pulled into this branch; zon fingerprint refreshed per the compiler for the renamed package. CLI naming settled: folder `agentsfleet/` (matches the pre-staged dotfiles audits; Indy final), package `@agentsfleet/cli`, bin `agentsfleet`. Entity noun zombie → agent approved — rides the follow-up mega-spec (M92_004: platform cutovers + wire/data/domain, expand-contract design doc in `docs/architecture/`); punch-list routing per Indy: one mega-spec, not three.
- **§1 ledger (Jun 12, 2026):** FLIP classes — CLI self-name strings (180 lines in `agentsfleet/{src,test}`; 24 path-like refs keep), daemon argv/usage fixtures + `logging.scoped(.zombied)` log scopes (grafana queries follow in §8), contract-lib + architecture-doc prose, website rendered copy naming the binary/package (vocab-guard/marketing-spec pins updated as conscious edits; the GitHub org slug and `/usezombie-install-platform-ops` keep), `public/llms.txt` + `public/skill.md` prose, telemetry service tags (internal dependency-injection ids, safe). KEEP classes — `docs/v1` + `docs/v2/done` history (append-only), directory-path refs, frozen `schema/*.sql` migration comments (editing frozen migrations breaks parity for zero operator gain — rides the schema cutover), fly/cloudflared/org/mail/header/user-db identifiers per the rename principle. UX note for Indy: the CLI config dir `~/.config/agentsfleet` flips clean to `~/.config/agentsfleet` — telemetry consent re-asked once; no fallback shim (RULE NLG). E3 keep baseline (pre-flip counts): `app = "zombied-`=4 · `core.zombie_`=305 · `usezombie-admin`=28 · `x-usezombie`=216 · `github.com/usezombie`=44.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification | Clean; outcome in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, the rename principle, Failure Modes, Invariants | Clean or dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Flip completeness | Eval `E1` | | |
| Build artifacts | Eval `E2` | | |
| Keep-pins | Eval `E3` | | |
| Make targets | Eval `E4` | | |
| Suite + integration | `make lint && make test && make test-integration` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | | |
| Workflows | Eval `E5` + CI run link | | |
| Images | Eval `E6` | | |
| Installer | Eval `E7` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- GitHub org rename (`github.com/usezombie`) and repo URLs — external resolver, own cutover spec.
- npm deprecation pass on the old `@usezombie/zombiectl` listing (deprecate-with-pointer once `@agentsfleet/cli` is stable) — registry janitorial, Indy-timed.
- fly app names (`zombied-dev`/`zombied-prod`), cloudflared hostname refs, live health URLs — platform identities traffic resolves against; own cutover.
- Directory paths (`agentsfleet/`, `src/agentsfleetd/`, `ui/agentsfleet.dev/`) — path churn with zero operator-visible gain this round.
- Postgres user/db (`usezombie-admin`), `core.zombie_*` schema, `x-usezombie*` headers — data-layer/API cutovers.
- `team@usezombie.com`, `api.usezombie.com` — mail/API cutovers, each its own spec.
- Dotfiles operating-model prose naming the old targets — companion dotfiles commit at cutover, not this repo's diff.
- Marketing copy/positioning — M92_001.
