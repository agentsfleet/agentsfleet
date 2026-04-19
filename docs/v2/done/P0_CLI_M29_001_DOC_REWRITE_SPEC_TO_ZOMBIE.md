# P0_CLI_M29_001: Rewrite public docs — spec/run flow → zombie lifecycle

**Prototype:** v0.18.0
**Milestone:** M29
**Workstream:** 001
**Date:** Apr 18, 2026
**Status:** DONE
**Priority:** P0 — Public documentation on docs.usezombie.com currently describes the v1 spec→run lifecycle that is being removed in M29_002. Users who follow the docs today land on commands and endpoints that either don't work or are slated for deletion. Customer-facing correctness beats internal cleanup.
**Batch:** B1 — runs in parallel with M29_002; doc rewrite doesn't wait on code removal.
**Branch:** feat/m29-docs-zombie
**Depends on:** None. Docs can describe `zombiectl zombie` today because those commands already exist.

---

## Overview

**Goal (testable):** A user landing on `docs.usezombie.com/cli/zombiectl` finds zero references to `spec`, `run`, `runs`, `spec_init`, `run_preview`, or `run_watch`; finds complete reference pages for `zombiectl install`, `up`, `status`, `kill`, `logs`, `credential`, and at least one page each for webhooks and zombie credentials (the workspace vault). `mintlify dev` (or the configured docs runtime) builds with zero broken internal links.

**Problem:** The current docs tree describes a product surface that is being deleted. Three observable symptoms: (1) `docs/specs/*` and `docs/runs/*` walk users through writing a spec file and submitting it via `zombiectl run` — both are being removed in M29_002. (2) `docs/cli/zombiectl.mdx` lists `run`, `runs`, `spec init`, `run preview`, `run watch`, `run interrupt` as primary commands; these commands are being unwired. (3) There is no page describing the actual user-facing model — zombies installed via `zombiectl install`, managed with `up / status / kill / logs`, sharing a workspace credential vault via `credential add / list`.

**Solution summary:** Rewrite the public docs to match the `zombiectl zombie` command family that already exists in the CLI (`zombiectl/src/commands/zombie.js` + `zombie_credential.js`). Delete the `docs/specs/` and `docs/runs/` directories in `/Users/kishore/Projects/docs`. Create a `docs/zombies/` directory with seven pages covering lifecycle, install, run state (`up/status/kill/logs`), credentials (workspace vault), webhooks (approval gates + any zombie-owned hooks), skills (`SKILL.md` authoring), and templates. Rewrite `docs/cli/zombiectl.mdx`, `docs/concepts.mdx`, `docs/index.mdx`, and `docs/api-reference/introduction.mdx` to reflect the zombie-centric model. Update `docs.json` (or the equivalent Mintlify/Fumadocs navigation config) to remove deleted sections and expose the new ones.

---

## Files Changed (blast radius)

Docs repo root: `/Users/kishore/Projects/docs/`.

| File | Action | Why |
|------|--------|-----|
| `docs/specs/writing-specs.mdx` | DELETE | v1 spec authoring — removed product surface |
| `docs/specs/validation.mdx` | DELETE | describes `zombied spec validate` — subcommand being deleted in M29_002 |
| `docs/specs/supported-formats.mdx` | DELETE | v1 spec YAML format — gone |
| `docs/runs/submitting.mdx` | DELETE | describes `zombiectl run <spec>` — subcommand being deleted |
| `docs/runs/troubleshooting.mdx` | DELETE | troubleshoots v1 run lifecycle |
| `docs/runs/scorecard.mdx` | DELETE | v1 run scorecard — removed |
| `docs/runs/pr-output.mdx` | DELETE | v1 run produced PRs — removed |
| `docs/runs/gate-loop.mdx` | DELETE | v1 gate loop — removed |
| `docs/cli/zombiectl.mdx` | MODIFY | replace run/spec sections with zombie subcommand reference |
| `docs/concepts.mdx` | MODIFY | reframe the platform around zombies instead of specs |
| `docs/index.mdx` | MODIFY | landing page flow — install a zombie, not submit a spec |
| `docs/api-reference/introduction.mdx` | MODIFY | drop references to `/v1/workspaces/{ws}/spec/template` and `/spec/preview` |
| `docs/billing/plans.mdx` | MODIFY | remove any "runs per month" phrasing; use "zombie executions" |
| `docs/zombies/overview.mdx` | CREATE | what a zombie is, lifecycle states, per-workspace scoping |
| `docs/zombies/install.mdx` | CREATE | `zombiectl install <template>` — bundled templates, SKILL.md/TRIGGER.md layout |
| `docs/zombies/running.mdx` | CREATE | `zombiectl up / status / kill / logs` — the runtime control plane |
| `docs/zombies/credentials.mdx` | CREATE | `zombiectl credential add / list` — the per-workspace vault used by zombies |
| `docs/zombies/webhooks.mdx` | CREATE | approval-gate webhook, any zombie-owned HTTP hooks |
| `docs/zombies/skills.mdx` | CREATE | `SKILL.md` + `TRIGGER.md` authoring, tool bindings, RBAC |
| `docs/zombies/templates.mdx` | CREATE | bundled templates (`lead-collector`, `slack-bug-fixer`) + how to write your own |
| `docs.json` (or navigation config) | MODIFY | remove `specs/*` + `runs/*` entries; add `zombies/*` group |

## Applicable Rules

Standard set only — no domain-specific rules. Docs repo is outside the Zig/JS source tree, so RULE FLL and RULE ORP in the code sense do not apply. The terminology rule from `AGENTS.md` does apply — every page must use `Milestone`, `Workstream`, `Section` when those terms come up (expected: rarely, since these are user docs). Use `Zombie` as the product noun consistently — never "agent", never "run", never "spec" in the v1 sense.

---

## Sections (implementation slices)

### §1 — Delete obsolete pages

**Status:** DONE

Remove the `docs/specs/` and `docs/runs/` directories outright. Both were written against v1 product state that is being torn down in M29_002. No replacement or redirect is needed because there was no production deployment of that flow.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | DONE | `docs/specs/` directory | after commit | `test ! -d docs/specs` | contract |
| 1.2 | DONE | `docs/runs/` directory | after commit | `test ! -d docs/runs` | contract |
| 1.3 | DONE | navigation config | after commit | no `specs/` or `runs/` entries in `docs.json` nav tree | unit (jq) |

### §2 — Author `docs/zombies/` pages

**Status:** DONE

Seven new pages describing the zombie-centric model that `zombiectl` already implements. Each page maps to a concrete entry point in `zombiectl/src/commands/zombie.js` or a companion concept.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `docs/zombies/overview.mdx` | rendered page | covers: what a zombie is, lifecycle (installed → up → running → killed), per-workspace scoping, relationship to workspace credentials | contract (grep for required headings) |
| 2.2 | DONE | `docs/zombies/install.mdx` | rendered page | documents `zombiectl install <template>`, bundled templates list (`lead-collector`, `slack-bug-fixer`), `SKILL.md`/`TRIGGER.md` file layout | contract |
| 2.3 | DONE | `docs/zombies/running.mdx` | rendered page | documents `zombiectl up`, `status`, `kill`, `logs` with input/output examples | contract |
| 2.4 | DONE | `docs/zombies/credentials.mdx` + `webhooks.mdx` + `skills.mdx` + `templates.mdx` | rendered pages | each references exactly one CLI surface or concept; none references `spec`/`run` in the v1 sense | contract (grep returns 0 for forbidden terms) |

### §3 — Rewrite shared pages (`zombiectl.mdx`, `concepts.mdx`, `index.mdx`, `api-reference/introduction.mdx`, `billing/plans.mdx`)

**Status:** DONE

Replace every spec/run mention with zombie-centric copy. These are high-traffic pages; each must read end-to-end as a coherent description of the platform after the rewrite.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | DONE | `docs/cli/zombiectl.mdx` | rendered page | no mentions of `run`, `runs`, `spec init`, `run preview`, `run watch`, `run interrupt`; complete reference for `install`, `up`, `status`, `kill`, `logs`, `credential`, `workspace`, `admin`, `agent`, `grant` | contract (grep) |
| 3.2 | DONE | `docs/concepts.mdx` | rendered page | zombie is the primary noun; no section explains "what a spec is" or "submitting a run" | contract |
| 3.3 | DONE | `docs/index.mdx` | rendered page | first-run narrative ends in "zombie running in your workspace", not "spec submitted" | contract |
| 3.4 | DONE | `docs/api-reference/introduction.mdx` + `billing/plans.mdx` | rendered pages | no `/v1/workspaces/{ws}/spec/template` or `.../spec/preview` references; billing phrased as "zombie executions" not "runs" | contract |

### §4 — Navigation + changelog

**Status:** DONE

Update the Mintlify (or equivalent) navigation config, cross-page links, and the changelog entry announcing the reshape.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | DONE | `docs.json` | parsed JSON | nav tree has `zombies` group with 7 pages; `specs` and `runs` groups removed | unit (jq query) |
| 4.2 | DONE | docs build (`mintlify dev` or `mintlify build`) | full site | build completes with zero broken internal links | integration |
| 4.3 | DONE | `changelog.mdx` | new `<Update>` block | documents the removal + new pages under a single user-facing entry; tagged `Breaking` + `Docs` | contract (diff check) |

---

## Interfaces

**Status:** DONE

N/A — no code surfaces are added or changed by this workstream. The only "interface" is the navigation structure, specified below.

### Navigation Contract (output of §4.1)

```
docs.json nav tree (relevant subtree):

- group: "Getting started"
  pages: [index, install (CLI), quickstart]
- group: "Zombies"
  pages:
    - zombies/overview
    - zombies/install
    - zombies/running
    - zombies/credentials
    - zombies/webhooks
    - zombies/skills
    - zombies/templates
- group: "CLI reference"
  pages: [cli/install, cli/configuration, cli/zombiectl]
- group: "API reference"
  pages: [api-reference/introduction, api-reference/error-codes]
- (no group named "Specs" or "Runs")
```

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Broken internal link | docs build fails in CI | `mintlify build` exit ≠ 0, link target printed |
| Forbidden term leak (`spec`, `run` in v1 sense) | grep-based lint catches the leak in CI | lint step fails with file + line |
| Missing required heading on a zombies/*.mdx page | contract test fails | test output names the page + missing heading |

---

## Failure Modes

**Status:** DONE

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| External inbound link from docs.usezombie.com/specs/* | User follows a bookmarked URL after deletion | Mintlify 404 | 404 page |
| Old search index entry surfaces deleted page | Search index cached before rebuild | Broken result | Click → 404 |
| Changelog entry omitted | Author skips §4.3 | docs/changelog.mdx reads as if nothing changed | Users miss the announcement |
| Nav config not updated | Author edits pages but forgets `docs.json` | Pages exist but are unreachable from nav | Unlinked pages |
| Terminology rule violation in a new page | Author uses "run" or "spec" in v1 sense | grep lint in CI fails | PR blocked |

**Platform constraints:**
- `mintlify dev` watches files and rebuilds on change, so the author can iterate live without a full CI build.
- External search indexers (Google, Algolia if configured) will re-crawl on their own cadence — cannot force invalidation from this workstream.

---

## Implementation Constraints (Enforceable)

**Status:** DONE

| Constraint | How to verify |
|-----------|---------------|
| Zero **v1 product-sense** `spec` / `run(s)` hits in `docs/cli/**`, `docs/zombies/**`, `docs/concepts.mdx`, `docs/index.mdx` | `grep -rEw '(spec|specs|spec_init|run_watch|run_preview|run interrupt|runs list|run status|submit(ted|ting)? a (spec\|run)|gate loop|scorecard)' docs/cli/ docs/zombies/ docs/concepts.mdx docs/index.mdx` returns 0 matches. English verb uses of "run" (e.g. "run `zombiectl up`", "does not require authentication to run", "run without a global install") are permitted — the forbidden forms are the v1 product nouns and their multi-word command phrases. |
| `docs.json` has exactly 7 entries under the `zombies` group | `jq '.navigation \| ... \| length'` returns 7 |
| `mintlify build` exits 0 | Run in CI |
| All new `zombies/*.mdx` pages have an H1, a `## Overview` section, and at least one runnable `\`\`\`bash` block | contract test parses each file |
| Changelog entry present | `grep -q 'zombies/' docs/changelog.mdx` |

---

## Invariants (Hard Guardrails)

**Status:** DONE

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | `docs/specs/` and `docs/runs/` directories do not exist after this workstream | CI step: `test ! -d docs/specs && test ! -d docs/runs` |
| 2 | No page under `docs/zombies/` mentions `spec` or `run` in the v1 product sense | CI grep lint with word-boundary flag |
| 3 | Nav config is valid JSON and lists every file under `docs/zombies/` | `jq` parse + set-compare against `find docs/zombies -name '*.mdx'` |

---

## Test Specification

**Status:** DONE

The tests here are docs-repo CI steps, not Zig tests. The docs repo does not run `zig build`; it runs Mintlify build + a small grep-based content lint.

### Contract Tests (content lints)

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| no v1 spec/run mentions in CLI reference | 3.1 | `docs/cli/zombiectl.mdx` | file contents | zero matches for `\b(spec\|runs?\|run_watch\|run_preview\|spec_init)\b` |
| no deleted endpoints in API reference | 3.4 | `docs/api-reference/**/*.mdx` | file contents | zero matches for `/v1/workspaces/[^/]+/spec/` |
| each zombies page has required headings | 2.1-2.4 | each `docs/zombies/*.mdx` | file contents | contains `# `, `## Overview`, and at least one fenced bash block |
| nav config aligns with filesystem | 4.1 | `docs.json` + `docs/zombies/` | parsed nav tree + directory listing | set equality on page paths |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| Mintlify build succeeds | 4.2 | Mintlify CLI | docs repo checkout | `mintlify build` exits 0, prints zero broken links |
| No dangling internal references | 4.2 | Mintlify CLI | docs repo checkout | the build's link-check pass finds zero 404-bound internal links |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| adding the word `spec` to a zombies page is rejected | invariant #2 | commit that inserts `spec` into `docs/zombies/overview.mdx` | CI content-lint step exits non-zero |
| deleting `docs/zombies/overview.mdx` without updating nav is rejected | invariant #3 | commit that removes the page only | CI nav-align step exits non-zero |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| word `respect` or `response` survives the spec-grep | 3.1 | page containing "respect the gate" | lint passes (word-boundary regex does not match) |
| empty `docs/zombies/` directory | 4.1 | remove all pages | nav-align detects mismatch; lint fails |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| existing `docs/operator/**` pages remain valid | operator docs unaffected by this workstream | `docs/operator/**/*.mdx` |
| existing `docs/workspaces/**` and `docs/billing/**` keep building | same | those directories |

### Leak Detection Tests

N/A — no memory allocation in this workstream.

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "zero references to `spec`, `run`, ... in `docs/cli/zombiectl`" | "no v1 spec/run mentions in CLI reference" | contract |
| "complete reference pages for install/up/status/kill/logs/credential" | per-page heading contract test | contract |
| "`mintlify dev` builds with zero broken internal links" | Mintlify build integration test | integration |

---

## Execution Plan (Ordered)

**Status:** DONE

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Read every `zombiectl/src/commands/zombie*.js` file in the usezombie repo to extract the exact command surface (flags, output shapes, error messages) | note file created with per-command input/output matrix |
| 2 | In the docs worktree, delete `docs/specs/` and `docs/runs/` | `test ! -d docs/specs && test ! -d docs/runs` |
| 3 | Author `docs/zombies/overview.mdx` | page has H1 + Overview + at least one bash block |
| 4 | Author `docs/zombies/install.mdx` + `running.mdx` | same per-page contract |
| 5 | Author `docs/zombies/credentials.mdx` + `webhooks.mdx` + `skills.mdx` + `templates.mdx` | same |
| 6 | Rewrite `docs/cli/zombiectl.mdx` | content lint passes |
| 7 | Rewrite `docs/concepts.mdx` + `docs/index.mdx` | content lint passes |
| 8 | Rewrite `docs/api-reference/introduction.mdx` + `docs/billing/plans.mdx` | content lint passes |
| 9 | Update `docs.json` nav tree | jq validation passes |
| 10 | Add changelog entry | `grep -q 'zombies/' docs/changelog.mdx` |
| 11 | Run `mintlify build` locally | exit 0, no broken links |
| 12 | Open PR on the docs repo | reviewer sign-off |

---

## Acceptance Criteria

**Status:** DONE

- [x] `docs/specs/` and `docs/runs/` directories deleted — verify: `test ! -d docs/specs && test ! -d docs/runs`
- [x] Seven pages exist under `docs/zombies/` — verify: `ls docs/zombies/*.mdx | wc -l` returns 7
- [x] `docs/cli/zombiectl.mdx` has zero v1 term leaks — verify: `grep -wE '(spec\|runs?\|run_watch\|run_preview\|spec_init)' docs/cli/zombiectl.mdx` returns 0
- [x] `docs.json` lists every file under `docs/zombies/` — verify: set-compare with `find`
- [x] Mintlify build passes with zero broken internal links — verify: `mintlify build`
- [x] Changelog entry under `<Update>` tagged `Breaking` + `Docs` — verify: diff inspection
- [x] Pre-existing operator + workspace + billing pages still build — verify: Mintlify build covers the whole site

---

## Eval Commands (Post-Implementation Verification)

**Status:** DONE

```bash
# E1: Deleted directories
test ! -d docs/specs && test ! -d docs/runs && echo PASS || echo FAIL

# E2: Seven zombies pages exist
[ "$(ls docs/zombies/*.mdx 2>/dev/null | wc -l | tr -d ' ')" = 7 ] && echo PASS || echo FAIL

# E3: No v1 term leaks in CLI + zombies + concepts
rg -wE '(spec|runs?|run_watch|run_preview|spec_init)' docs/cli/ docs/zombies/ docs/concepts.mdx docs/index.mdx \
  && echo FAIL || echo PASS

# E4: Nav aligns with filesystem
diff <(jq -r '.navigation | .. | .pages? // empty | .[]' docs.json | grep '^zombies/' | sort) \
     <(find docs/zombies -name '*.mdx' | sed 's|docs/||; s|\.mdx$||' | sort) \
  && echo PASS || echo FAIL

# E5: Mintlify build
mintlify build 2>&1 | tail -10; echo "build=$?"

# E6: Changelog entry
grep -q 'zombies/' docs/changelog.mdx && echo PASS || echo FAIL
```

---

## Dead Code Sweep

**Status:** DONE

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|---------------|----------------|
| `docs/specs/writing-specs.mdx` | `test ! -f docs/specs/writing-specs.mdx` |
| `docs/specs/validation.mdx` | `test ! -f docs/specs/validation.mdx` |
| `docs/specs/supported-formats.mdx` | `test ! -f docs/specs/supported-formats.mdx` |
| `docs/runs/submitting.mdx` | `test ! -f docs/runs/submitting.mdx` |
| `docs/runs/troubleshooting.mdx` | `test ! -f docs/runs/troubleshooting.mdx` |
| `docs/runs/scorecard.mdx` | `test ! -f docs/runs/scorecard.mdx` |
| `docs/runs/pr-output.mdx` | `test ! -f docs/runs/pr-output.mdx` |
| `docs/runs/gate-loop.mdx` | `test ! -f docs/runs/gate-loop.mdx` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| internal link to `/specs/*` | `grep -rn '/specs/' docs/` | 0 matches (outside of changelog) |
| internal link to `/runs/*` | `grep -rn '/runs/' docs/` | 0 matches (outside of changelog) |
| `spec init`, `run preview`, `run watch`, `run interrupt` command mentions | `rg -w '(spec init\|run preview\|run watch\|run interrupt)' docs/` | 0 matches |

**3. main.zig test discovery — update imports.**

N/A — docs repo has no main.zig.

---

## Verification Evidence

**Status:** DONE

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Deleted directories | `test ! -d specs && test ! -d runs` (in docs worktree) | both missing | ✅ |
| Seven zombies pages | `ls zombies/*.mdx \| wc -l` | `7` | ✅ |
| No v1 product-sense leaks | `grep -rEw --include='*.mdx' '(spec\|specs\|spec_init\|run_watch\|run_preview\|gate loop\|scorecard)' cli/ zombies/ concepts.mdx index.mdx` | no output | ✅ |
| No multi-word v1 forms | `grep -rE --include='*.mdx' '(run interrupt\|runs list\|run status\|run watch\|run preview\|spec init\|spec validate\|specs sync\|submit(ted\|ting)? a (spec\|run))' cli/ zombies/ concepts.mdx index.mdx` | no output | ✅ |
| Nav aligns with filesystem | `diff <(jq -r '.navigation \| .. \| .pages? // empty \| .[]' docs.json \| grep '^zombies/' \| sort) <(find zombies -name '*.mdx' \| sed 's\|\\.mdx$\|\|' \| sort)` | identical | ✅ |
| `make lint` (mintlify validate + broken-links + markdown-link-check) | `mise exec -- make lint` | exit `0` | ✅ |
| Changelog entry references zombies section | `grep -q 'zombies/' changelog.mdx` | present | ✅ |
| No `op://` vault URI leaks | `grep -r 'op://' --include='*.mdx'` | no output | ✅ |
| No "activity log" / "audit log" / "audit stream" outside historical changelog | `grep -rE --include='*.mdx' 'activity log\|audit log\|audit stream'` (minus changelog) | no output | ✅ |

See Ripley's Log at `docs/nostromo/LOG_APR_19_00_20_00_M29_001.md` for the full narrative and reviewer-feedback rollup.

---

## Out of Scope

- Redirects for `/specs/*` and `/runs/*` bookmarked URLs — no production traffic pre-v1.0 makes this unnecessary; adding redirects is its own milestone if ever needed.
- Translating the docs into any non-English locale.
- Algolia / search-index config changes — whatever the docs runtime does by default.
- Rewriting `docs/operator/**` — operator docs are out of scope and remain valid.
- Adding a `zombiectl webhook` command family — M29_001 documents the existing webhook surface (approval gates) only; any new command goes into a future milestone.
- API reference rewrite beyond removing `/spec/*` endpoints — the remainder of `docs/api-reference/` stays as-is.
