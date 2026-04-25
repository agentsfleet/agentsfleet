# HANDOFF — Apr 25, 2026: 10:13 PM

**Branch:** `docs/v2-spec-backlog`
**Scope:** Documentation rewrite + v2 spec backlog from M40-M51 + global TEMPLATE.md / AGENTS.md updates.
**Status:** Ready for review. No code shipped this session — pure docs + specs + cleanup.

---

## TL;DR

This session re-grounded the v2 backlog after the wedge framing (`platform-ops` GH Actions CD-failure responder + chat steer) was sharpened in office-hours. The deleted `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` was the canonical reference for 5 stale pending specs; rather than restore it, the session:

1. **Rewrote `docs/ARCHITECHTURE.md`** as the new canonical reference (770 lines, 13 sections + glossary).
2. **Deleted 10 stale pending specs** that grounded against the deleted doc.
3. **Wrote 12 fresh specs M40-M51** in `docs/v2/pending/` from a rewritten `docs/TEMPLATE.md` that enforces a goal-contract registry over implementation pseudocode.
4. **Codified the skill-driven review chain** (`/write-unit-test` → `/review` → `/review-pr`) in both `docs/TEMPLATE.md` and the global `AGENTS.md` CHORE(close).
5. **Wired the "Applicable Rules" bridge** so every spec lists which rule files (`docs/ZIG_RULES.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, `docs/greptile-learnings/RULES.md`) apply to its scope — turning soft trigger-based rule-reading into explicit per-spec required reading.

**Wedge framing locked:** `platform-ops` zombie that wakes on GH Actions `workflow_run.conclusion=failure`, gathers evidence via `http_request` against fly.io / upstash / GitHub APIs / Slack, posts evidenced diagnosis. Same zombie also reachable via `zombiectl steer {id}`. **No database-operations / migrating-zombie content** — that drift was removed everywhere.

---

## What changed

### Repo-side (this branch)

| File / dir | Change | Lines |
|---|---|---|
| `docs/ARCHITECHTURE.md` | NEW — canonical v2 reference (was previously a placeholder) | 760L |
| `docs/TEMPLATE.md` | EDITED via dotfiles — goal-contract template, Implementing Agent prologue, Applicable Rules section (top), Anti-Patterns moved to top, Skill-Driven Review Chain section | 397L (was 480) |
| `docs/REST_API_DESIGN_GUIDELINES.md` | REWRITTEN — instruction-shaped (Quick Checklist + 13 sections + SDK note), section numbering re-anchored | 475L (was 538) |
| `docs/v2/pending/M40_001_WORKER_SUBSTRATE.md` | NEW | 140L |
| `docs/v2/pending/M41_001_CONTEXT_LAYERING.md` | NEW (renamed from "Execution Substrate" per user direction) | 165L |
| `docs/v2/pending/M42_001_STREAMING_SUBSTRATE.md` | NEW | 284L |
| `docs/v2/pending/M43_001_WEBHOOK_INGEST.md` | NEW | 216L |
| `docs/v2/pending/M44_001_INSTALL_CONTRACT_AND_DOCTOR.md` | NEW | 205L |
| `docs/v2/pending/M45_001_VAULT_STRUCTURED_CREDENTIALS.md` | NEW | 250L |
| `docs/v2/pending/M46_001_FRONTMATTER_SCHEMA.md` | NEW | 228L |
| `docs/v2/pending/M47_001_APPROVAL_INBOX.md` | NEW (database-ops drift removed) | 227L |
| `docs/v2/pending/M48_001_BYOK_PROVIDER.md` | NEW | 202L |
| `docs/v2/pending/M49_001_INSTALL_SKILL.md` | NEW | 312L |
| `docs/v2/pending/M50_001_ARCHITECTURE_CROSS_REFERENCE.md` | NEW | 172L |
| `docs/v2/pending/M51_001_DOCS_AND_INSTALL_PINGBACK.md` | NEW | 249L |
| `docs/office_hours_v2.md` | NEW — copy of gstack design doc from `~/.gstack/projects/usezombie/` | 186L |
| `docs/plan_engg_review_v2.md` | NEW — copy of gstack eng-review test plan from `~/.gstack/projects/usezombie/` | 105L |
| `playbooks/ARCHITECHTURE.md` | NEW — tunnel-first architecture rationale (architecture-only, zero playbook references) | 151L |
| `playbooks/README.md` | EDITED — link to `playbooks/ARCHITECHTURE.md` |  |
| `docs/v2/pending/P1_*` (10 stale specs) | DELETED |  |
| `docs/v2/surfaces.md` | DELETED — pre-pivot positioning |  |
| `docs/tunnel-first-architecture.md` | DELETED — content split into `playbooks/ARCHITECHTURE.md` (architecture only); startup-thesis content dropped (preserved in git history) |  |
| `docs/nostromo/LOG_*.md` (21 files) | DELETED |  |
| `docs/nostromo/slack_*.md` (4 files) | DELETED — pre-pivot positioning (Slack as input trigger; reality is Slack as output channel only) |  |
| `samples/homebox-audit/`, `samples/migration-zombie/`, `samples/side-project-resurrector/` | DELETED — stale stubs |  |

`samples/` now has only `platform-ops/` (the wedge sample).

### Dotfiles-side (already committed + pushed in earlier commits)

These edits hit symlinked files under `~/.claude/`, which resolve to `~/Projects/dotfiles/`. Per the global rule, they were committed and pushed in-session.

| Commit | Files | Summary |
|---|---|---|
| `8a8d0e8` | `docs/TEMPLATE.md` | Goal-contract registry rewrite (480L → 320L) |
| `5f1bafd` | `AGENTS.md` | REST guidelines reference renumbered |
| `87aa31b` | `AGENTS.md`, `docs/TEMPLATE.md` | Skill-driven review chain codified |
| `1ac0bcb` | `AGENTS.md`, `docs/TEMPLATE.md` | Applicable Rules section + Anti-Patterns moved to top |

GitHub: `indykish/dotfiles@master` is at `1ac0bcb`.

---

## Wedge framing (locked)

```
TRIGGER:  GitHub Actions workflow_run.conclusion=failure
            → POST /v1/.../webhooks/github (HMAC-verified)
            → synthetic event on zombie:{id}:events with actor=webhook:github

ALSO REACHABLE:  zombiectl steer {id} "<message>"
                   → SET zombie:{id}:steer; worker GETDELs at top of loop
                   → synthetic event with actor=steer:<user>

REASONING LOOP:  one runner.execute call inside zombied-executor sandbox
                   → NullClaw agent makes http_request tool calls
                   → tool bridge substitutes ${secrets.NAME.FIELD} after sandbox entry
                   → posts evidenced diagnosis to Slack
                   → core.zombie_events row updated with response + tokens

NOT IN WEDGE:  Database operations / migrating-zombie / approval-gated
               destructive ops as a flagship — explicitly removed from
               docs/ARCHITECHTURE.md §4.2, §8.6, §1, §7. Approval gates
               (M47) still ship for tool-call cost overruns + future
               zombie shapes; not framed as a parallel wedge.

WEDGE v3:  Bastion / customer-facing statuspage. Documented in
           docs/ARCHITECHTURE.md §13. Post-launch.
```

---

## Spec backlog — M40-M51 (revised Apr 25, 2026 by /plan-ceo-review)

Tier breakdown after the CEO scope review:

| Tier | Specs |
|---|---|
| **Launch-blocking substrate (Week 1-3)** | M40 Worker, M41 Context Layering, M42 Streaming, M43 Webhook Ingest, M44 Install Contract + Doctor, M45 Vault Structured, **M48 BYOK Provider** (promoted from "soft-blocks" tier — see decision §A below) |
| **Launch-shipping packaging (Week 4-7)** | M46 Frontmatter Schema, M49 Install-Skill, M51 Docs + Install-Pingback (now also owns architecture cross-reference + ship reflection — see fold §B below) |
| **Parallel from Week 1, validation-blocking** | **M50 Customer-Development Parallel Workstream** (NEW — see decision §C below). ~2-3 hours/week founder time alongside substrate. |
| **Post-launch** | M47 Approval Inbox |

**Total: 11 specs (down from 12 after the M50→M51 fold), milestone scope ~6-7 weeks (up from 5-6 after BYOK promotion).**

### §A — M48 BYOK promoted to substrate-tier (Apr 25 /plan-ceo-review decision)

With self-host deferred to v3 (decided earlier in the same session), the v2 differentiation pillars compressed from 4 to 3: **OSS + BYOK + markdown-defined**. The /plan-ceo-review surfaced that dropping BYOK leaves 2 pillars, both matchable by competitors (AWS DevOps Agent / Sonarly / incident.io) within a week. M48 moves from "Soft-blocks BYOK launch claim, post-launch" to "Launch-blocking substrate, Week 2-3." Adds ~1 week to the milestone. Differentiation argument holds at 3 pillars.

### §B — M50 folded into M51 (Apr 25 /plan-ceo-review decision)

M50 was originally "ARCHITECHTURE.md cross-reference + post-launch reflection" — meta-work without independent user value. Folded into M51 (which already owned docs.usezombie.com positioning, install-pingback, and privacy doc). Same workstream owner, same release rhythm. M51 absorbs M50's §1-§5 as its new §4.1-§4.6, plus adds a README hero sync deliverable + launch-tweet copy freeze. Spec count drops 12 → 11; no content lost. The M50 slot was vacated and immediately reassigned to M50 Customer-Dev (§C).

### §C — M50 Customer-Development Parallel Workstream (NEW, Apr 25)

Codex eng-review correctly identified that "3 named installs in 2 weeks" measured setup-pain-tolerance, not demand. Day-50 bar revised to "1 external team kept install on for ≥1 week, real production event." But the original plan started customer-dev at Week 6 (post-ship), giving 14 days from cold outreach to install-and-soak. /plan-ceo-review surfaced this compression as a real failure mode: ship into silence, find out the framing was wrong, burn 5-6 weeks before the signal arrives. M50 Customer-Dev makes operator-discovery a first-class parallel workstream from Week 1. Founder time: ~2-3 hours/week. Goal: 3 named operators committed to Day-35 install slots BEFORE Day 35; 1 of 3 converts by Day 50. Concierge installs OK.

### What didn't change

- Substrate-first sequencing (Codex was right — don't re-litigate)
- Day-50 validation bar (1 team, real event, ≥1 week)
- Wedge framing (GH Actions CD-failure responder + manual operator steer)
- M37_001 platform-ops sample as the wedge artifact

Codex outside-voice review (in `docs/office_hours_v2.md`) flagged 8 issues, 3 P0s. The biggest: M19_003 + M37_001 substrate mismatches — install API contract broken, parser key mismatch (`tools` vs `skills`), worker doesn't pick up new zombies without restart, executor doesn't pass tool config or network policy. M44 + M40 + M41 fix all of these.

---

## Open decisions (NOT yet acted on)

The user explicitly held three pending items in flight; the next agent should decide and execute:

1. **Rollback plan section in TEMPLATE.md** — required for breaking-change / schema-touching specs. *Status: discussed; not added.* If approved, add a new template section between "Failure Modes" and "Invariants" for revert procedures.

2. **Observability ask section in TEMPLATE.md** — required for handler/runtime specs naming the logs/metrics/traces the implementation must add. *Status: discussed; not added.* If approved, add a new section between "Interfaces" and "Failure Modes".

3. **Backfill "Applicable Rules" into M40-M51** — current specs reference rule files implicitly (in Acceptance Criteria) but not in the explicit "Applicable Rules" section the new TEMPLATE.md requires. *Status: not done.* If approved, walk each spec, add the section, list which of (`ZIG_RULES.md`, `REST_API_DESIGN_GUIDELINES.md`, `greptile-learnings/RULES.md`, `SCHEMA_CONVENTIONS.md`) and which specific rule IDs apply.

---

## Next concrete steps (recommended order)

1. **Read this handoff + `docs/ARCHITECHTURE.md`** in full.
2. **Decide on the 3 open items above** (rollback section, observability section, M40-M51 backfill).
3. **Run `/plan-ceo-review`** on the M40-M51 backlog. The substrate scope expanded 4× when Codex's findings landed; that's the right moment for a strategy sanity check before committing 5-6 weeks of substrate work.
4. **CHORE(open) for M40-M45 substrate work** (per office-hours D5 sequencing decision):
   ```bash
   git checkout main
   git branch feat/m40-substrate
   git worktree add ../usezombie-m40-substrate feat/m40-substrate
   cd ../usezombie-m40-substrate
   # Move M40-M45 from pending/ to active/
   for n in 40 41 42 43 44 45; do
     git mv docs/v2/pending/M${n}_001_*.md docs/v2/active/
   done
   # Set Status: IN_PROGRESS, Branch: feat/m40-substrate in each spec header
   # Commit on the feature branch
   ```
5. **Implement substrate** following the new template's review chain:
   - Read each spec's Implementing-agent prologue + Applicable Rules
   - Code → invoke `/write-unit-test` → invoke `/review` → CHORE(close) → `gh pr create` → `/review-pr`
6. **By Day 50** (per design doc's revised validation bar): 1 external team using platform-ops on a real production event for ≥1 week. Concierge installs OK.

---

## Reference points (where things live)

| What | Where |
|---|---|
| Canonical architecture (problem, thesis, runtime model, agent↔zombie steer flow, capabilities, context lifecycle, 11-step technical sequence, path to bastion) | `docs/ARCHITECHTURE.md` (760L) |
| Tunnel-first deployment architecture (rationale only, no playbook content) | `playbooks/ARCHITECHTURE.md` (151L) |
| Spec template + skill-driven review chain + Applicable Rules + Anti-Patterns | `docs/TEMPLATE.md` (symlinked → dotfiles) |
| REST API design guidelines (instruction-shaped) | `docs/REST_API_DESIGN_GUIDELINES.md` (475L, 13 sections) |
| Zig prescriptive rules | `docs/ZIG_RULES.md` |
| Cross-language project rules | `docs/greptile-learnings/RULES.md` |
| v2 active milestone backlog | `docs/v2/pending/M40_001_*.md` through `M51_001_*.md` (12 specs, 2,650L total) |
| Office-hours design doc (the wedge decision) | `docs/office_hours_v2.md` (186L) |
| Eng-review test plan | `docs/plan_engg_review_v2.md` (105L) |
| Agent operating model (global) | `~/.claude/CLAUDE.md` → symlinked to `~/Projects/dotfiles/AGENTS.md` |
| Wedge sample | `samples/platform-ops/` |

---

## Skill-driven review chain (the canonical order)

Codified in dotfiles `AGENTS.md` CHORE(close) and in `docs/TEMPLATE.md`:

```
implement
   ↓
/write-unit-test    (gate: clean output / iteration count in Ripley's Log)
   ↓
/review             (gate: clean output OR all findings dispositioned)
   ↓
CHORE(close)        (move spec done/, Ripley's Log, changelog, version sync)
   ↓
gh pr create
   ↓
/review-pr          (gate: PR comments addressed before human review / merge)
   ↓
human review + merge
```

If a skill is unavailable (MCP server down): document the skip explicitly in the Ripley's Log AND the PR description with timestamp + "rerun before merge".

---

## Cautions / known gotchas

1. **`docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` is deleted.** 5 of the 10 deleted pending specs cited it. The new `docs/ARCHITECHTURE.md` §12 ("End-to-End Technical Sequence") restores the 11-step content from that doc. New specs (M40-M51) cite `docs/ARCHITECHTURE.md`, not the deleted file.

2. **Spec naming convention is the new format.** Files use `M{N}_{NNN}_{NAME}.md` — priority + categories live in the spec header, not the filename. Old specs used `P{N}_{CATS}_M{N}_{NNN}_{NAME}.md` and were deleted. The user-pending naming convention chore (mentioned in office-hours) was NOT filed as a spec — it's implicit (the new naming is what M40-M51 uses).

3. **`samples/migration-zombie/`, `samples/migrating-zombie/`, `samples/homebox-audit/` are gone.** The architecture doc no longer mentions a "second validating workflow." M47 spec uses an internal test fixture at `samples/fixtures/m47-gate-fixture/` — NOT a public sample.

4. **`zombiectl install` auth-exempt is a P0 that lives in shipped code today** (`zombiectl/src/cli.js:36`). M44 fixes it. Until M44 ships, install requests bypass local auth check, hit the server, and fail with confusing 401s.

5. **The wedge depends on M33 + M34 + M35 + install-contract fixes ALL landing before the install-skill (M49) can demo end-to-end.** Codex flagged this; the sequencing decision (5A in office-hours D5) is "substrate-first, package second."

6. **`usezombie.sh` CDN must be live before launch** per A2 decision in office-hours. Cloudflare CDN config is the only blocker for the skill-publication URL `https://usezombie.sh/skills/install-platform-ops/SKILL.md`.

7. **No code changes in this session.** Everything is docs + specs + cleanup. Nothing was committed to `main`.

---

## Branch ready to push (if you want)

This branch (`docs/v2-spec-backlog`) has not been pushed to a remote. To push:

```bash
git push -u origin docs/v2-spec-backlog
```

Or open a PR via `gh pr create --base main --head docs/v2-spec-backlog --title "docs(v2): re-ground architecture + write M40-M51 spec backlog"`.

---

## End of handoff

Next agent: read `docs/ARCHITECHTURE.md` first. Then this file. Then decide on the 3 open items. Then `/plan-ceo-review` on the backlog.
