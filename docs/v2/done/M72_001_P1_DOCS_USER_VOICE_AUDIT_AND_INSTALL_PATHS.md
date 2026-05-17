<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M72_001: User-Voice Audit + Install-Path Parity for docs.usezombie.com

**Prototype:** v2.0.0
**Milestone:** M72
**Workstream:** 001
**Date:** May 17, 2026
**Status:** DONE
**Priority:** P1 — operator-facing docs drift from architectural source-of-truth; cold-start install instructions undersell the simpler path.
**Categories:** DOCS
**Batch:** B1
**Branch:** chore/m72-docs-user-voice-audit-changelog (own-branch on `<docs-repo>/`, per AGENTS.md docs-repo flow)
**Depends on:** None. M68_001 (trigger DX + free trial) and M49_001 (install-skill) already DONE — this spec reconciles the docs against what shipped.
**Provenance:** human-written (Kishore, May 17, 2026) — audit driven by direct request, drift surfaced manually against `docs/architecture/*.md`.

**Canonical architecture:** `docs/architecture/user_flow.md` §8.0 / §8.2.1 / §8.7, `high_level.md` §1, `direction.md`, `capabilities.md` §1.

---

## Implementing agent — read these first

1. `docs/architecture/user_flow.md` — the canonical user-side narrative. §8.0 (wedge surface), §8.2.1 (cold-machine bootstrap with both install paths), §8.2.2 (per-zombie install flow), §8.7 (model + posture origin). The spec's reconciliation targets all derive from this file.
2. `docs/architecture/high_level.md` §1–§4 — product framing. Sets the lead voice ("Operational outcomes do not fall into limbo") and the three structural pillars (open source / self-managed provider keys / markdown-defined). Docs lead today with the noun "always-on agent runtime" instead of the user-facing promise.
3. `docs/architecture/capabilities.md` §1 — the binding distinction between `SKILL.md` (advisory prose) and `TRIGGER.md` (executor-enforced). Concepts page currently calls a tool a "Skill" — a category error against this doc.
4. `<docs-repo>/` — the user-facing docs repo. Walk every file listed in *Files Changed* before editing. The Mintlify changelog voice rules in `<docs-repo>/docs/CHANGELOG_VOICE.md` apply to the changelog entry that lands with this milestone.
5. `docs/v2/done/M68_001_P1_API_CLI_DOCS_UI_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` and `M49_001_P1_SKILL_DOCS_INSTALL_SKILL.md` — what shipped. Spec amendments to the docs must match the shipped behavior, not the pre-ship plan.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal coding/docs discipline. Specifically RULE NDC (no dead code/links) and RULE NLR (touch-it-fix-it cleanup): when an MDX file gets touched for the audit, fix unrelated voice/stale-claim issues in the same file rather than leaving partial work.
- `<docs-repo>/docs/CHANGELOG_VOICE.md` — Mintlify changelog voice. The `<Update>` entry that lands with this milestone must follow the headline + lead-paragraph + `**Bold lead-noun**` bullet structure; banned vocabulary list applies.
- `<docs-repo>/AGENTS.md` — docs-repo agent contract. Confirm before editing.
- `docs/SCHEMA_CONVENTIONS.md`, `docs/ZIG_RULES.md`, `docs/REST_API_DESIGN_GUIDELINES.md` — N/A. DOCS-only spec; no code paths touched.

---

## Overview

**Goal (testable):** Every page under `<docs-repo>/` that names an install path lists both `npx skills add usezombie/usezombie` (the one-liner) and the `https://usezombie.sh/skills.md` curl fallback in that order, and every page under `<docs-repo>/` opens with the user-facing problem ("a deploy fails at 2am, evidence is scattered, the on-call engineer is asleep") before introducing the platform's nouns — verified by per-file grep assertions in §Acceptance Criteria.

**Problem:** Three observable drifts between the architecture source-of-truth and the published docs:

1. **Voice drift.** `index.mdx`, `concepts.mdx`, and `zombies/overview.mdx` lead with platform-internal nouns ("always-on agent runtime", "preconfigured agent process", "the four nouns"). The architecture's user-facing promise — *operational outcomes do not fall into limbo* — surfaces only in the second or third paragraph, if at all. A user landing cold cannot tell within ten seconds what problem the product solves for them.

2. **Install-path drift.** `cli/install.mdx`, `quickstart.mdx`, and `cli/zombiectl.mdx` show one install path for the host-agent skill: `curl https://usezombie.sh/skills.md > ~/.claude/skills/.../SKILL.md`. Architecture `user_flow.md` §8.2.1 prescribes two paths in the cold-machine bootstrap — `npx skills add usezombie/usezombie` (one-liner, symlinks the slash command into every supported host's skill path) as the primary, plus the manual curl as the fallback. Users following the published docs do the manual path when the one-liner would have worked.

3. **Concept drift.** `concepts.mdx` calls a tool (`http_request`, `memory_store`) a "Skill" — directly contradicting `capabilities.md` §1, which binds `SKILL.md` to *prose policy* and tools to *executor-enforced primitives*. `cli/install.mdx` describes `doctor` as a three-check command (`server_reachable`, `workspace_selected`, `workspace_binding_valid`); `user_flow.md` §8.2.2 adds `auth_token_present` and the `tenant_provider` block (mode + model + context_cap_tokens + M68 free-trial state). `concepts.mdx` Trigger accordion still shows the pre-migration webhook URL `https://api.usezombie.com/v1/webhooks/{zombie_id}` (no source suffix) — the M43/M68 migration has already shipped at `{zombie_id}/{source}` everywhere else.

**Solution summary:** One reconciliation pass over the docs repo. Each touched file gets three things in order: (1) opening reframed to lead with the user's problem, then the platform's response, then the nouns; (2) install commands updated to list both paths with the one-liner first; (3) stale claims (doctor checks, webhook URL shape, tool-vs-skill distinction, lifecycle verbs, posture-flip flow) corrected against the named architecture sections. The spec ships as one PR against `<docs-repo>/`, with a single Mintlify `<Update>` entry summarising the user-visible changes. No code paths touched.

---

## Files Changed (blast radius)

> All paths relative to `<docs-repo>/` unless otherwise noted.

| File | Action | Why |
|------|--------|-----|
| `index.mdx` | EDIT | Reframe hero (problem-first), reorder pillars to match `high_level.md` §1, fix Architecture card link target. |
| `quickstart.mdx` | EDIT | Replace single-path "Install and run the install skill" step with two-path (npx primary, curl fallback). Reword doctor preconditions to match `user_flow.md` §8.2.2. |
| `concepts.mdx` | EDIT | Rename "the four nouns" framing to three-primaries + tool/skill distinction. Fix Trigger accordion webhook URL to `{zombie_id}/{source}`. Replace "Skill = named tool" with the binding `SKILL.md` (advisory) vs `TRIGGER.md` (enforced) split. |
| `cli/install.mdx` | EDIT | Add `npx skills add usezombie/usezombie` as the primary install path; promote curl to fallback. Expand doctor checks table to include `auth_token_present` and `tenant_provider` (M68 free-trial-aware). |
| `cli/zombiectl.mdx` | EDIT | Mirror install-path update in the install/cold-start section. Add `tenant provider set` and `tenant provider show` subcommand stubs that point at the self-managed walkthrough. |
| `zombies/overview.mdx` | EDIT | Add the missing `stop` and `resume` states to the Lifecycle mermaid (currently only 3 states; `zombies/running.mdx` already documents 5 verbs). Re-lead with "outcome ownership" framing. |
| `zombies/install.mdx` | EDIT | Mirror the two-path install update for the host-agent skill. Cross-link to the self-managed posture walkthrough. |
| `zombies/templates.mdx` | EDIT | Reconcile "template" vs "sample" vocabulary — architecture uses "sample"; pick one and use it consistently across this page and `zombies/install.mdx`. |
| `zombies/authoring.mdx` | EDIT | Audit-pass only — confirm `SKILL.md` framing matches `capabilities.md` §1; no large rewrite expected. |
| `memory.mdx` | EDIT | Audit-pass only — confirm category framing matches what shipped in M14_001; correct any stale claims. |
| `changelog.mdx` | EDIT | New `<Update>` entry (top of file), one headline + lead paragraph + bullets per `CHANGELOG_VOICE.md`. |
| `docs/v2/pending/M72_001_*.md` | CREATE | This spec. |

---

## Sections (implementation slices)

### §1 — Voice reframing pass

Every user-facing entry point (`index.mdx`, `quickstart.mdx`, `concepts.mdx`, `zombies/overview.mdx`) leads with the user's problem in plain prose, then names the platform's response. The architecture's anchor phrase is *"Operational outcomes do not fall into limbo"* (`high_level.md` §4). The hero on `index.mdx` opens with the deploy-at-2am scenario (already present, currently buried below the noun-led headline), promotes it to the lead paragraph, then introduces the platform.

**Implementation default:** Keep the existing scenario prose where it appears — reorder, don't rewrite. The architecture's voice is already correct in many lead paragraphs; the drift is in the headlines and the noun-first framing. Reorder, then trim.

### §2 — Install-path parity

Every page that shows an install command for the host-agent skill lists both paths in the same order: `npx skills add usezombie/usezombie` first (primary, one-liner), `curl -fsSL https://usezombie.sh/skills.md > ~/.claude/skills/.../SKILL.md` second (fallback, when the user's host does not expose a node toolchain or when they want to inspect the file before installing).

**Implementation default:** Use Mintlify `<Tabs>` for the two paths where both fit naturally (quickstart Step 5, install.mdx, zombies/install.mdx). For inline references (index.mdx hero card), use the primary path only and link to the fuller install page.

The user-visible claim each touched page makes: *"`npx skills add usezombie/usezombie` symlinks the `/usezombie-*` slash commands into your host's skill directory (Claude Code, Amp, Codex CLI, OpenCode). Pass `--host=<name>` to pin a specific host."* — exact flag name and behaviour to be confirmed against the npx package's actual CLI surface before merge; if the package ships a different flag, update the docs and this spec.

### §3 — Concept reconciliation against `capabilities.md`

`concepts.mdx` is the single most-linked-to internal page. Its drift compounds. Three targeted fixes:

1. **Tool vs skill.** Remove the "Skill" card from the four-nouns grid. Replace with a "Tool" card pointing at the binding distinction: tools are platform primitives (`http_request`, `memory_store`, `cron_add`) declared in `TRIGGER.md` and executor-enforced; `SKILL.md` is *prose policy* the model reads as advisory system prompt. Reference `capabilities.md` §1.
2. **Webhook URL shape.** The Trigger accordion currently shows `https://api.usezombie.com/v1/webhooks/{zombie_id}` (no source suffix). M43/M68 migrated this to `{zombie_id}/{source}` and the other docs are already updated. Fix this one occurrence.
3. **Doctor checks.** The `cli/install.mdx` doctor table is missing `auth_token_present` and `tenant_provider` (with M68 free-trial state). Update against `user_flow.md` §8.2.2.

### §4 — Lifecycle parity

`zombies/overview.mdx` lifecycle mermaid shows three states (Alive / Processing / Killed). `zombies/running.mdx` correctly documents the five verbs (`stop`, `resume`, `kill`, `delete`, plus `install`). Update the overview diagram to match. The verb ladder is already canonical in `zombies/running.mdx` — overview is the stale one; mirror it.

### §5 — Posture-flip surface

`user_flow.md` §8.7 documents the platform-managed vs self-managed posture and the `zombiectl tenant provider set` flip. The published docs hint at "you bring your provider and model" but never show the flip command or explain when a user would run it. Add a short "Bringing your own provider key" section to `cli/zombiectl.mdx` (or a dedicated `/cli/tenant-provider.mdx` if scope warrants — defer to the implementing agent's read of the current page sizes). Link from `index.mdx` pillar card and `concepts.mdx` credit section.

**Implementation default:** Section, not new page — the flip is a single CLI invocation plus a short explanation of when a user wants it. Defer the dedicated page until the self-managed flow gains additional commands.

### §6 — Changelog entry

One Mintlify `<Update>` at the top of `<docs-repo>/changelog.mdx`. Headline + lead paragraph + 3–5 `**Bold lead-noun** — consequence-first` bullets per `CHANGELOG_VOICE.md`. The user-visible change is: docs now lead with the operational-outcome problem, install instructions show the one-liner first, and the posture-flip surface is documented. Internal cleanup (audit pass, vocabulary reconciliation) gets aggressive trimming or omission per voice rules.

---

## Interfaces

DOCS-only spec — no HTTP, CLI, or RPC contracts added or changed. The interfaces the spec *documents* (and must not invent) are already shipped:

```
# Install paths (both shipped via M49_001):
npx skills add usezombie/usezombie [--host=<claude|amp|codex|opencode>]
curl -fsSL https://usezombie.sh/skills.md -o <host-skill-path>/SKILL.md

# Doctor (shipped via M68_001, extended by M48b):
zombiectl doctor [--json]
  returns: { ok, api_url, checks: [
    { name: "auth_token_present", ok: bool },
    { name: "server_reachable", ok: bool },
    { name: "workspace_selected", ok: bool },
    { name: "workspace_binding_valid", ok: bool },
    { name: "tenant_provider", ok: bool, data: { mode, model, context_cap_tokens, free_trial_state? } }
  ] }

# Webhook URL (shipped via M43/M68):
POST https://api.usezombie.com/v1/webhooks/{zombie_id}/{source}

# Tenant provider flip (shipped via M48_001):
zombiectl tenant provider set --credential <credential-name>
zombiectl tenant provider show [--json]
```

If the implementing agent finds any of these shapes differ from what is currently shipped (CLI help output, OpenAPI, code), the spec is wrong — surface the divergence in the Discovery log and update the spec before continuing.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `npx skills add` flag mismatch | The published npm package's CLI surface differs from what `user_flow.md` §8.2.1 implies (e.g., flag is `--client` not `--host`, or the package name is different). | Run `npx skills add --help` against the live package before merging. If the doc claim cannot be supported by the published binary, update the spec interfaces section and the doc text. Do not invent a flag. |
| Mintlify build break | A reordered hero or new `<Tabs>` block breaks docs.json navigation or component rendering. | Run `mintlify dev` locally before commit; the build must render with no errors. CI surfaces broken links and component errors. |
| Stale-claim cascade | The audit surfaces a stale claim that requires a code/architecture fix, not a docs fix (e.g., a doctor check name the code uses differs from arch). | STOP and surface to Captain. Architecture Consult Guard fires — the doc wins until reconciled, but if arch and code disagree, this is a separate spec. Do not patch the doc to match incorrect code. |
| `<Update>` entry voice drift | The changelog entry slips into marketing language banned by `CHANGELOG_VOICE.md` ("seamless", "powerful", "magical"). | Lint-pass before commit. The voice rules ship with a grep allowlist; fail closed. |
| Cross-repo write friction | This spec lives in the usezombie repo but the edits land in the `usezombie/docs` repo. Implementer must use the docs repo's own branch flow. | Per `AGENTS.md` Operational defaults: in the docs repo, `git status`, branch off main (`chore/m72-docs-user-voice-audit-changelog` or similar), commit there. The spec stays in usezombie; the docs PR carries the audit. |

---

## Invariants

1. **Install-path order is enforced by grep.** Any page that mentions both install paths lists `npx skills add` before the `curl` fallback. Verified by §Acceptance Criteria grep — no human review required.
2. **No invented CLI surface.** Every command shown in the docs corresponds to a real shipped command in `zombiectl` or the published `skills` npx package. Verified by running each `<command> --help` in the implementing agent's local environment as part of CHORE(close).
3. **Voice-leading rule.** The first heading-or-paragraph of every touched page mentions either the user's problem or the user's outcome before naming a platform noun (`zombie`, `runtime`, `agent`, `workspace`, `tenant`). Verified by manual review against the §Acceptance Criteria checklist; not lint-enforceable.

---

## Test Specification

> No code is shipped, so "tests" are verification commands run against the rendered docs and the diff.

| Test | Asserts |
|------|---------|
| `test_install_path_primary_npx` | Every file in `Files Changed` that names a skill install command contains `npx skills add usezombie/usezombie` before any `curl https://usezombie.sh/skills.md`. |
| `test_no_unsuffixed_webhook_url` | No published page contains `webhooks/{zombie_id}` without a trailing `/{source}` or `/{url_secret}` segment. |
| `test_concepts_skill_distinction` | `concepts.mdx` contains both the strings "advisory" (referring to `SKILL.md`) and "enforced" (referring to `TRIGGER.md`) within 50 lines of each other. |
| `test_doctor_check_names_complete` | `cli/install.mdx` doctor checks table contains all five names: `auth_token_present`, `server_reachable`, `workspace_selected`, `workspace_binding_valid`, `tenant_provider`. |
| `test_overview_lifecycle_complete` | `zombies/overview.mdx` mermaid contains all five states (`Alive`, `Processing`, `Stopped`, `Killed`, plus an `install` entry transition). |
| `test_voice_lead_rule` | Manual review checklist (one row per touched page): first paragraph mentions a user problem or outcome before any platform noun. Recorded in the PR Session Notes. |
| `test_changelog_voice_compliance` | New `<Update>` entry in `changelog.mdx` passes the `CHANGELOG_VOICE.md` banned-vocabulary grep and has the headline-plus-lead-plus-bullets shape. |
| `test_mintlify_build` | `mintlify dev` (or `mintlify build`) completes with no errors or broken-link warnings on the touched pages. |

---

## Acceptance Criteria

- [ ] Every file in *Files Changed* (except the spec itself) has been edited per §1–§6, with the diff scoped to voice / install paths / stale claims — no opportunistic refactors. Verify: `git diff --stat origin/main -- ':(exclude)docs/v2/pending/'` lists exactly the files in the table.
- [ ] Install-path primary-first ordering. Verify: `grep -rn 'usezombie.sh/skills.md\|skills add usezombie' <docs-repo>/*.mdx <docs-repo>/zombies/*.mdx <docs-repo>/cli/*.mdx` shows `skills add` line numbers strictly lower than `skills.md` line numbers in every file that contains both.
- [ ] No stale webhook URL. Verify: `! grep -rn 'webhooks/{zombie_id}"' <docs-repo>/*.mdx <docs-repo>/zombies/*.mdx <docs-repo>/cli/*.mdx | grep -v '/{source}\|/{url_secret}'`
- [ ] Doctor table complete. Verify: `grep -c 'auth_token_present\|server_reachable\|workspace_selected\|workspace_binding_valid\|tenant_provider' <docs-repo>/cli/install.mdx` returns `5`.
- [ ] Mintlify build clean. Verify: `cd "$DOCS_REPO" && mintlify build 2>&1 | grep -E 'error|warning' | head` empty.
- [ ] Changelog entry lands at the top of `changelog.mdx`, passes voice lint. Verify: `head -50 <docs-repo>/changelog.mdx | grep -E 'seamless\|powerful\|magical\|robust'` empty.
- [ ] Each command shown in the published docs has been confirmed against `--help` output of the corresponding shipped binary. Recorded in PR Session Notes one-per-command.
- [ ] No file in the docs repo grows past 350 lines as a result of these edits.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: install-path primary-first
cd "$DOCS_REPO" && for f in $(grep -l 'usezombie\.sh/skills\.md' index.mdx quickstart.mdx zombies/*.mdx cli/*.mdx 2>/dev/null); do
  a=$(grep -n 'skills add usezombie' "$f" | head -1 | cut -d: -f1)
  b=$(grep -n 'usezombie\.sh/skills\.md' "$f" | head -1 | cut -d: -f1)
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -gt "$b" ] && echo "FAIL $f: skills.md at $b before skills add at $a"
done

# E2: no unsuffixed webhook URL
cd "$DOCS_REPO" && grep -rn 'webhooks/{zombie_id}"' *.mdx zombies/*.mdx cli/*.mdx 2>/dev/null \
  | grep -v '/{source}\|/{url_secret}' \
  && echo "FAIL: stale webhook URL" || echo "PASS"

# E3: doctor checks complete
cd "$DOCS_REPO" && for c in auth_token_present server_reachable workspace_selected workspace_binding_valid tenant_provider; do
  grep -q "$c" cli/install.mdx || echo "FAIL: missing doctor check $c"
done

# E4: Mintlify build clean
cd "$DOCS_REPO" && mintlify build 2>&1 | tail -20

# E5: 350-line gate
cd "$DOCS_REPO" && wc -l index.mdx quickstart.mdx concepts.mdx zombies/*.mdx cli/*.mdx changelog.mdx \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'

# E6: changelog voice lint
head -60 <docs-repo>/changelog.mdx \
  | grep -iE 'seamless|powerful|magical|robust|effortless|game-chang' \
  && echo "FAIL: banned vocabulary in new changelog entry" || echo "PASS"

# E7: shipped-CLI surface check
which zombiectl && zombiectl --help | head -40
which npx && npx skills --help 2>&1 | head -20
```

---

## Dead Code Sweep

No files deleted by this spec. N/A — no orphaned references expected.

If §3 (tool-vs-skill reconciliation) introduces a renamed internal anchor (e.g., `concepts.mdx#skill` → `concepts.mdx#tool`), grep the docs repo for inbound links and update them in the same diff. Verify: `grep -rn '/concepts#skill\|/concepts/skill' <docs-repo>/` returns zero after the edit.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | DOCS spec — runs as a structural audit: does every claim in §Test Specification have a verifying grep/build check? No code tests are generated. | Audit report attached to PR Session Notes. |
| After audit pass, still before CHORE(close) | `/review` | Adversarial pass against `docs/architecture/*.md` — every claim in the touched docs must trace to an architecture section. Catches voice drift and invented CLI surface. | Findings dispositioned (fix / defer with reason). |
| After `gh pr create` | `/review-pr` | Re-runs the review against the immutable PR diff. Catches anything `/review` missed once the changelog entry and final ordering land. | Comments addressed before requesting human review. |

---

## Discovery (consult log)

Empty at creation. Populated as the implementing agent surfaces architecture / legacy consults during EXECUTE.

---

## Verification Evidence

> Filled in during VERIFY phase. Proves spec claims are met.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Install-path order (E1) | see Eval Commands | | |
| Webhook URL shape (E2) | see Eval Commands | | |
| Doctor checks (E3) | see Eval Commands | | |
| Mintlify build (E4) | `mintlify build` | | |
| 350-line gate (E5) | `wc -l` | | |
| Changelog voice (E6) | grep | | |
| CLI surface real (E7) | `--help` | | |

---

## Out of Scope

- **Code changes.** Any drift that turns out to require a code or architecture fix (CLI flag rename, doctor check addition, OpenAPI shape change) becomes a separate spec. This one ships docs against what is already shipped.
- **New dedicated pages.** §5 (posture-flip surface) is added as a section, not a new `/cli/tenant-provider.mdx` page. Promoting it to its own page is a follow-up once the self-managed flow grows additional commands.
- **Template marketplace copy.** `zombies/templates.mdx` mentions a future hosted marketplace in a `<Note>`; this spec does not expand that. Marketplace docs land when the marketplace ships.
- **Architecture-doc edits.** The architecture is the source-of-truth for this spec. If a doc claim cannot be supported by the architecture, the doc loses — the architecture is not amended by this milestone.
- **Localised / i18n copy.** docs.usezombie.com is English-only today. The voice reframing pass stays English-only.
- **Tutorial / Diataxis restructure.** This is a reconciliation pass, not a rewrite. Diataxis-style restructuring (tutorial vs how-to vs reference vs explanation) is a separate milestone.
