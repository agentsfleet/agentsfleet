# M97_001: Route website post-deploy smoke to the canonical prod domain

**Prototype:** v2.0.0
**Milestone:** M97
**Workstream:** 001
**Date:** Jun 23, 2026
**Status:** DONE
**Priority:** P1 — a production CI gate (`smoke (post-deploy)` website job) is red on every marketing deploy, drowning real signal.
**Categories:** INFRA
**Batch:** B1 — standalone CI fix, no dependents.
**Branch:** feat/m97-001-website-smoke-canonical
**Test Baseline:** unit=2015 integration=201
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, CI failure triage of runs 27975813182 + 27975833257).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `playbooks/ARCHITECTURE.md` — defines the Vercel-project → host topology (app / website / agents) the smoke gate routes against. CI workflow shape is greenfield in `docs/architecture/`; the workflow file itself is the source of truth.

---

## Implementing agent — read these first

1. `.github/workflows/smoke-post-deploy.yml` — the workflow being edited; the `Detect project` step already branches per Vercel project (app / website / agents) and the smoke job consumes `target_url` as `BASE_URL`. Mirror that branch structure.
2. `ui/packages/website/tests/e2e/smoke.spec.ts` — the smoke suite; its header comment already prescribes the canonical post-deploy target (`BASE_URL=https://agentsfleet.net`).
3. `ui/packages/website/playwright.config.ts` — shows the `x-vercel-protection-bypass` header wiring; explains why the protected generated URL needs a bypass the canonical domain does not.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(ci): smoke website against canonical prod domain, not protected deploy URL
- **Intent (one sentence):** the website post-deploy smoke gate turns green by testing the user-facing canonical domain (`agentsfleet.net`), which is unprotected, instead of the Vercel-protected generated deployment URL that returns a "Login – Vercel" page.
- **Handshake (agent fills at PLAN, before EXECUTE):** ASSUMPTIONS I'M MAKING: (1) Vercel Standard Protection guards generated `*.vercel.app` deployment URLs but leaves the promoted production custom domain open; (2) on a `deployment_status == success` for a Production deploy, Vercel has already promoted the build to the canonical alias, so smoking the alias tests the just-shipped build; (3) only the **website** project's canonical domain is a marketing HTML site safe to smoke — `agentsfleet.dev` serves the installer script, and the **app** project's generated-URL+bypass path already passes, so neither changes.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a maintainer pushes the marketing site to production; minutes later the `smoke (post-deploy)` website job is green, confirming the live site renders its title, nav, pricing, and legal pages — no false red to triage.
2. **Preserved user behaviour** — the app project's smoke (generated URL + `x-vercel-protection-bypass`) and the acceptance-e2e-prod job stay exactly as they are; no change to what or how they test.
3. **Optimal-way check** — the most direct path is to point the website smoke at the domain real users hit. Unconstrained-optimal would also fix the website's automation-bypass secret so the generated URL works; that is a Vercel/1Password config change out of repo scope, and testing the canonical domain is the better semantic regardless.
4. **Rebuild-vs-iterate** — iterate. A workflow rewrite would trade away the working app/agents paths for no gain; determinism is preserved by a per-project URL branch.
5. **What we build** — a per-project smoke `BASE_URL` selection in `smoke-post-deploy.yml`: website → `https://agentsfleet.net`; app and agents → unchanged (`target_url`).
6. **What we do NOT build** — no fix to the `vercel-bypass-website` secret (out of repo); no change to acceptance-e2e-prod (`api.agentsfleet.net` DNS is an intentional infra gap — prod backend not deployed); no change to the app smoke.
7. **Fit with existing features** — compounds with the existing post-deploy gate; must not destabilize the app smoke or the acceptance job.
8. **Surface order** — CI-only; no CLI/UI surface.
9. **Dashboard restraint** — N/A — no UI.
10. **Confused-user next step** — a maintainer seeing the website smoke fail reads the job log; the canonical-domain `BASE_URL` is printed by the `Detect project` step, making the target self-evident.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (always applies); specifically **NLR** (touch-it-fix-it: the `Detect project` step is edited, keep it coherent) and **NLG** (no new legacy framing).
- No language dispatch façades apply — the diff touches a single GitHub Actions YAML file, no `*.zig` / `*.ts` / `schema/*` / `src/http/handlers/**`.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` touched |
| PUB / Struct-Shape | no | no public code surface |
| File & Function Length (≤350/≤50/≤70) | no | single YAML file, well under caps |
| UFS (repeated/semantic literals) | yes | the canonical host `https://agentsfleet.net` appears once in the workflow; keep it single-sourced in the `Detect project` step, not duplicated across steps |
| UI Substitution / DESIGN TOKEN | no | no UI code |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | none of these surfaces touched |

---

## Overview

**Goal (testable):** on a Production website deploy, the `smoke (post-deploy)` website job runs the smoke suite against `https://agentsfleet.net` and all 11 specs pass (no "Login – Vercel" title).

**Problem:** the website post-deploy smoke job fails on every Production marketing deploy. It navigates to the Vercel-generated deployment URL (`agentsfleet-website-*.vercel.app`), which Vercel deployment protection gates with an "Authentication Required" / "Login – Vercel" page; the website project's `x-vercel-protection-bypass` secret is not accepted, so the suite never reaches the site and `toHaveTitle(/agentsfleet/i)` fails.

**Solution summary:** in `smoke-post-deploy.yml`, the `Detect project` step emits a per-project `smoke_base_url`. For the website project it is the canonical, unprotected production domain `https://agentsfleet.net`; for app and agents it remains the deployment `target_url`. The smoke step consumes `smoke_base_url` as `BASE_URL`. User-visible outcome: the website gate reflects the live production site and turns green.

---

## Prior-Art / Reference Implementations

- **CI** → the existing `Detect project` per-project branch in the same workflow is the pattern to mirror; the change extends each branch with one `smoke_base_url` output rather than introducing a new mechanism.
- The website smoke suite's own header comment (`ui/packages/website/tests/e2e/smoke.spec.ts`) documents the canonical post-deploy target, so this aligns the workflow with already-stated intent rather than inventing a convention.

No new architecture; shape defined by the existing workflow.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `.github/workflows/smoke-post-deploy.yml` | EDIT | add `smoke_base_url` per-project output in `Detect project`; consume it as `BASE_URL` in the smoke step |
| `docs/v2/pending/M97_001_P1_INFRA_WEBSITE_SMOKE_CANONICAL_DOMAIN.md` | CREATE | this spec |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a single Section — extend the existing detect branch with one output and thread it to the smoke step.
- **Alternatives considered:** (a) fix the `vercel-bypass-website` automation-bypass secret so the generated URL is reachable — rejected: out-of-repo Vercel/1Password config, and testing the protected artifact is a weaker semantic than testing the live canonical site; (b) disable Vercel protection on the website project — rejected: protection is desired for preview deploys, and this is a Vercel-dashboard change, not code.
- **Patch-vs-refactor verdict:** this is a **patch** because the workflow's per-project routing is already correct; only the smoke target for one project is wrong. No follow-up refactor is owed.

---

## Sections (implementation slices)

### §1 — Per-project canonical smoke target

The `Detect project` step gains a `smoke_base_url` output set per detected project: website → the canonical production domain; app and agents → the deployment `target_url` (current behaviour preserved). The smoke step uses `smoke_base_url` as `BASE_URL`. This delivers the green website gate while leaving the working app/agents paths untouched.

- **Dimension 1.1** — DONE — website project resolves `smoke_base_url` to the canonical production domain → Test `test_website_smoke_targets_canonical_domain`
- **Dimension 1.2** — DONE — app and agents projects resolve `smoke_base_url` to the deployment `target_url` (no behavioural change) → Test `test_app_agents_smoke_keep_target_url`
- **Dimension 1.3** — DONE — the website smoke suite passes end-to-end against the canonical production domain → Test `test_website_smoke_green_against_canonical`

---

## Interfaces

```
Workflow: .github/workflows/smoke-post-deploy.yml
  job: smoke
    step "Detect project" (id: detect) — new output:
      smoke_base_url:
        package=website  -> https://agentsfleet.net
        package=app      -> ${{ github.event.deployment_status.target_url }}
        package=agents   -> ${{ github.event.deployment_status.target_url }}
    step "Run smoke tests against deployed URL":
      env.BASE_URL = ${{ steps.detect.outputs.smoke_base_url }}
```

The contract: the smoke job's `BASE_URL` is sourced from `steps.detect.outputs.smoke_base_url`, never directly from `target_url`.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Protected deploy URL | website smoke hits a Vercel-gated `*.vercel.app` URL | fixed: website now targets the unprotected canonical domain; suite reaches the real site |
| Empty `smoke_base_url` | a detect branch omits the new output | `BASE_URL` would be empty and Playwright would error on navigation; every detect branch sets `smoke_base_url`, so no branch falls through |
| App/agents regression | the change accidentally alters app/agents targets | app and agents branches explicitly set `smoke_base_url` to `target_url`; their behaviour is unchanged |
| Canonical not yet promoted | smoke runs before Vercel promotes the alias | gate fires on `deployment_status == success` for Production, which is post-promotion; the alias reflects the shipped build |

---

## Invariants

1. Every `Detect project` branch sets `smoke_base_url` — enforced by the smoke step reading only `steps.detect.outputs.smoke_base_url`; a missing branch output yields an empty `BASE_URL` that fails the job loudly (no silent skip).
2. App and agents smoke targets are unchanged — enforced by those branches setting `smoke_base_url` to the same `target_url` value previously used as `BASE_URL`.

---

## Test Specification (tiered)

> The "tests" for a CI workflow change are the workflow's own observable behaviour: the detect step's emitted output and a real run of the smoke suite against the chosen target. There is no unit-test harness for GitHub Actions YAML in this repo; the e2e proof is running the suite locally against the canonical domain.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `test_website_smoke_targets_canonical_domain` | website branch of `Detect project` emits `smoke_base_url=https://agentsfleet.net` |
| 1.2 | e2e | `test_app_agents_smoke_keep_target_url` | app + agents branches emit `smoke_base_url` equal to `github.event.deployment_status.target_url` |
| 1.3 | e2e | `test_website_smoke_green_against_canonical` | `BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts` → 11 passed |

Regression: N/A — the app/agents smoke and acceptance-e2e-prod jobs are explicitly unchanged; their behaviour is preserved by setting their target to the prior value. Idempotency/replay: N/A — no retry semantics introduced.

---

## Acceptance Criteria

- [ ] website smoke passes against the canonical domain — verify: `cd ui/packages/website && BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts` → `11 passed`
- [ ] workflow YAML is valid — verify: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/smoke-post-deploy.yml'))" && echo OK`
- [ ] app/agents branches still target `target_url` — verify: `grep -n "smoke_base_url" .github/workflows/smoke-post-deploy.yml`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: website smoke green against canonical domain
( cd ui/packages/website && BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts ) && echo "PASS" || echo "FAIL"
# E2: workflow YAML parses
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/smoke-post-deploy.yml'))" && echo "PASS"
# E4: per-project smoke_base_url wired
grep -n "smoke_base_url" .github/workflows/smoke-post-deploy.yml
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted.

---

## Discovery (consult log)

- **Consult — Indy (Jun 23, 2026):** scoping of the acceptance-e2e-prod failure (`api.agentsfleet.net` NXDOMAIN). Indy decision: *"I think let it fail, since we havent deployed prod yet."* — context: acceptance-e2e-prod is excluded from this spec's scope; the prod API backend/tunnel DNS is an intentional infra gap, not a code bug.
- **Consult — Indy (Jun 23, 2026):** canonical-domain mapping. Indy: *"canonical domain for prod is agentsfleet.net(website), and agentsfleet.dev(agents) app.agentsfleet.net(app) its not deployed yet i think, i may be wrong."* — context: verified `agentsfleet.dev` serves the installer bash script (not marketing HTML), so only the **website** project is retargeted to its canonical domain; app/agents stay on `target_url`.
- **Skill chain outcomes:**
  - `/write-unit-test` (Jun 23, 2026): diff ledger 3/3 resolved — website branch covered by e2e playwright smoke vs `agentsfleet.net` (11 passed); app/agents branch `won't-test` (value identical to the prior inlined `BASE_URL`, zero behaviour change); `BASE_URL` output wiring covered by actionlint + smoke run. No supported unit-test stack applies to inline-YAML bash; a `bats` test would duplicate implementation logic (anti-pattern). Zig-leak / concurrency / perf proofs N/A — no code. Verdict: no unit tests warranted.
  - `/review` (Jun 23, 2026): adversarial diff review CLEAN — no critical findings. One informational: app/agents branches round-trip `target_url` through `GITHUB_OUTPUT` (theoretical newline-injection; `target_url` is a trusted Vercel HTTPS URL, pre-existing trust, not blocking). Invariants 1 + 2 verified; "canonical not yet promoted" failure mode handled by the post-promotion `deployment_status==success` gate.
  - `/review-pr`, `kishore-babysit-prs` — recorded below as they run.
- **Deferrals** — none.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Website smoke (e2e) | `BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts` | `11 passed (8.9s)` | ✅ |
| Workflow YAML valid | `python3 -c "import yaml; yaml.safe_load(...)"` | `PASS` | ✅ |
| actionlint + make-target refs | `make check-gh-actions-valid` | `actionlint + make-target refs all green` | ✅ |
| Playbooks ref integrity | `make check-playbooks` | `all references resolve` / `README documents every playbook dir` | ✅ |
| Gitleaks | `gitleaks protect --staged` (pre-commit) | `no leaks found` | ✅ |

---

## Out of Scope

- Fixing the `vercel-bypass-website` automation-bypass secret so the generated deployment URL is reachable — Vercel/1Password config, not repo code.
- acceptance-e2e-prod / `api.agentsfleet.net` DNS — intentional infra gap; prod API backend not deployed (Indy: "let it fail").
- The app and agents smoke targets — unchanged.
