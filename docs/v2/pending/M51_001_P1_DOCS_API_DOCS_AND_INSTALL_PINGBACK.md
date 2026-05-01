# M51_001: docs.usezombie.com Positioning Rewrite + Install-Pingback Endpoint + Architecture Cross-Reference

**Prototype:** v2.0.0
**Milestone:** M51
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — packaging-blocking. The launch tweet links to `docs.usezombie.com/quickstart/platform-ops`; if it 404s or shows stale homelab-zombie content, the launch lands flat. The install-pingback is what gives us the Day-N install metric without DM-polling Twitter.
**Categories:** DOCS, API
**Batch:** B3 — depends on all other v2 substrate + packaging being shippable. Final milestone before launch.
**Branch:** feat/m51-docs-and-pingback (to be created)
**Depends on:** M40-M49 (substrate + packaging — cross-reference pass walks every shipped spec). Nothing structural for the pingback endpoint itself.
**Folded in:** M50 (architecture cross-reference + post-launch reflection) — formerly a separate spec, merged here Apr 25, 2026 because the docs workstream owns documentation drift, M50 was meta-work without independent user value, and consolidating reduces milestone count.

**Canonical architecture:** `docs/architecture/` §0, §3 (positioning) + §11 (context lifecycle — needs a user-facing doc). This spec also keeps `docs/architecture/` itself accurate post-ship (formerly M50's job).

---

## Cross-spec amendment (Apr 30, 2026 — folded from M43 review pass)

The /quickstart/platform-ops walkthrough and the /privacy/cli-telemetry doc both touch surfaces that the M43 webhook review pinned. Three reinforcements:

**D1 — Quickstart step 6 wording.** The current draft (§2 step 6: "Set up the GH webhook: copy the URL + secret the skill emits, paste into the GH repo's webhook settings.") matches the post-M43 design. Concretize the URL: the user pastes `https://api.usezombie.com/v1/webhooks/{zombie_id}`. The secret is the value the install-skill (M49) generated and showed once during install — not stored anywhere user-visible after that moment. The doc explicitly says: "Lost the secret? Run `zombiectl credential add github --data='{\"webhook_secret\":\"<new-S>\", ...}'` to rotate."

**D2 — Workspace-scoped webhook credential.** The quickstart must show that one operator at one workspace pastes the same secret into N repo webhook configs (one per zombie covering N repos). This is the actually-simple operator UX that the workspace-credential design unlocks; the doc should say so plainly. Tradeoff (also document): rotation is workspace-wide; rotating affects every zombie in the workspace.

**D3 — Privacy doc /privacy/cli-telemetry stays unchanged.** Webhook secrets are not telemetry. The pingback endpoint collects only `{install_id, skill, skill_version, os, ts}`. No webhook URL, no secret bytes, no zombie_id. Verify in the audit acceptance criterion (already present at line 277).

No file additions or removals from §M51 §Files Changed table.

---

## Implementing agent — read these first

1. `~/Projects/docs/` — the docs.usezombie.com source repo. Read its existing structure (likely Mintlify or similar): `mint.json`, navigation tree, hero copy, existing pages.
2. `docs/architecture/` (this repo) — the canonical reference; the docs site is the user-facing version of relevant sections.
3. M49's spec (sibling) for the install-skill flow — `/quickstart/platform-ops` walks through this.
4. Vercel / Cloudflare project configs (whichever hosts docs.usezombie.com today) — know how the site deploys.
5. Existing privacy doc patterns — Turso, Resend, PlanetScale all have CLI telemetry privacy pages worth mirroring for tone.

---

## Overview

**Goal (testable):** Operator visits `https://docs.usezombie.com` and sees:

1. **Hero**: *"Durable, BYOK, markdown-defined agent runtime — for operators who own their outcomes."* — replaces any "AI for SREs" framing. Three differentiation pillars: OSS + BYOK + markdown-defined. Free hosted; open source; **self-host arrives in v3**.
2. **`/quickstart/platform-ops`** — single page walking through `/usezombie-install-platform-ops` (same name in every host) from agent installation through first Slack post. Includes screenshots and a short screen recording.
3. **`/skills`** — describes the `usezombie-*` skill family (`usezombie-install-platform-ops` for now; future `usezombie-steer`, `usezombie-doctor`) and the single install procedure: drop the SKILL.md directory into the host's skills folder or fetch via `usezombie.sh`.
4. **`/concepts/context-lifecycle`** — user-facing version of §11 in architecture/. Includes the L1+L2+L3 ASCII diagram and the override table.
5. **`/privacy/cli-telemetry`** — privacy contract for the install-pingback (what we collect, what we don't, opt-out flag).

Plus: `POST https://api.usezombie.com/v1/skills/install-pingback` is live, accepting anonymous install events from the install-skill (M49). Returns 204 No Content. Daily aggregates visible in an internal dashboard.

**Plus (folded from M50):** `docs/architecture/` cross-referenced against shipped specs and updated with a §14 ship reflection. Every `(M{N})` mention in the architecture doc points at a real spec in `docs/v2/done/`. §14 captures what shipped vs planned, what surprised us, what was deferred.

**Launch-tweet copy freeze:** 48h before ship date, the launch tweet copy + landing-page hero + first-screenshot are signed off against the architecture doc's §0 differentiation pillars. Catches the moment the tweet drifts from the substrate truth (e.g., accidentally claims self-host).

**Problem:** The current docs site (`~/Projects/docs/`) still talks about homelab-zombie and a kubectl-first narrative that no longer ships. If the launch tweet links to it, readers see a contradiction with the tweet's claim. There's also no install metric — Day-50 validation depends on knowing if anyone installed. Separately, the architecture doc was rewritten BEFORE substrate shipped; predictions in it (e.g., "M43 owns webhook ingest") are guesses until reconciled.

**Solution summary:** Three parallel deliverables. (1) Docs site rewrite — positioning + 5 new pages, deprecate stale ones. (2) Server-side install-pingback endpoint that the install-skill POSTs to anonymously after a successful install. Privacy-first: anonymized install ID (random UUID stored locally, not user/repo identity), skill version, timestamp, OS family. No repo names, no email, no token, no IP retention beyond aggregation. (3) Architecture cross-reference + §14 ship reflection — keeps the canonical doc honest post-ship.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/index.mdx` (or equivalent) | EDIT | Hero copy rewrite |
| `~/Projects/docs/quickstart/platform-ops.mdx` | NEW | Install + first-chat walkthrough |
| `~/Projects/docs/skills/index.mdx` | NEW | Skill catalog overview |
| `~/Projects/docs/skills/usezombie-install-platform-ops.mdx` | NEW | Detail page for the install skill |
| `~/Projects/docs/concepts/context-lifecycle.mdx` | NEW | User-facing context layering doc |
| `~/Projects/docs/privacy/cli-telemetry.mdx` | NEW | Pingback privacy contract |
| `~/Projects/docs/mint.json` (or nav config) | EDIT | Add new pages to nav. **Do NOT add a Self-Host nav entry** — self-host is v3. |
| `~/Projects/docs/integrations/lead-collector.mdx` | DELETE if exists | Stale homelab-era content |
| `~/Projects/docs/launch/homelab-zombie.mdx` | DELETE if exists | Same |
| `~/Projects/docs/self-host.mdx` | DO NOT CREATE | Self-host deferred to v3; no v2 page (removed from M51 scope Apr 25, 2026) |
| `README.md` (root, this repo) | EDIT | Hero line synced to architecture §0 differentiation pillars |
| `~/Projects/.github/README.md` (org-level GitHub profile) | EDIT | Same hero line, public-facing |
| `docs/architecture/` | EDIT | Cross-reference correctness pass + §14 ship reflection (folded from M50) |
| `src/http/handlers/skills/install_pingback.zig` | NEW | The pingback endpoint |
| `src/http/router.zig` | EXTEND | Wire `/v1/skills/install-pingback` |
| `src/state/install_metrics.zig` | NEW | Aggregation: count by day, skill version, OS family |
| `tests/integration/install_pingback_test.zig` | NEW | E2E: POST → 204; aggregation increments |

> **Cross-repo PR**: docs site changes are in a different repo (`~/Projects/docs/`). Coordinate the merge timing with the main repo's launch.

---

## Sections (implementation slices)

### §1 — Hero copy + nav rewrite

Hero copy on landing page leads with: *"Durable, BYOK, markdown-defined agent runtime — for operators who own their outcomes."* Sub-line: *"Free hosted. Open source. Self-host arrives in v3."* Below the fold: 30-second install demo (short screen recording or animated GIF) of `/usezombie-install-platform-ops` running.

> **Implementation default:** mirror the visual rhythm of Resend.com or Turso.com docs — punchy hero, tight code snippet, three-card differentiation block, then the quickstart link. No marketing fluff.

Navigation: top-level entries reorganized to:
- Quickstart
- Concepts (incl. context lifecycle)
- Skills
- API Reference
- Privacy

(No `Self-Host` nav entry — self-host is v3. The `/self-host` URL intentionally 404s in v2; see Out of Scope and `test_no_self_host_page_in_v2`.)

### §2 — `/quickstart/platform-ops`

Single page, top-to-bottom walkthrough:

1. Prerequisite: Claude Code (or Amp, Codex CLI, OpenCode) installed locally. `zombiectl` installed (link to install).
2. Run `zombiectl auth login` (signs in via Clerk OAuth).
3. Run `/usezombie-install-platform-ops` in any supported host (Claude Code, Amp, Codex CLI, OpenCode).
4. Answer the 3 prompts (Slack channel, branch glob, cron opt-in). BYOK setup is a separate later step if you want to bring your own key — see the BYOK page.
5. Skill installs the zombie + posts a first response to Slack.
6. Set up the GH webhook: copy the URL + secret the skill emits, paste into the GH repo's webhook settings.
7. Trigger: cause a deploy failure → see the Slack diagnosis arrive.

Include real screenshots (or a short screen recording). Author's repo as the demo target.

### §3 — `/concepts/context-lifecycle`

User-facing version of §11 in architecture/. Same ASCII diagram. Same override table. Add: a "common questions" section addressing things like "do I need to tune these?" (answer: no, defaults work), "what if my zombie needs more depth?" (answer: bump `tool_window` first; everything else is fine for 95% of cases).

### §4 — Architecture cross-reference + §14 ship reflection (folded from M50)

Three small passes after the substrate ships, before launch tweet goes out:

**§4.1 Cross-reference correctness pass.** For each spec referenced in `docs/architecture/` (M40-M49):
1. Confirm the spec is in `docs/v2/done/` (not still pending or active).
2. Read the spec's `## Overview`; verify the capability description in architecture/ §10 (Capabilities table) matches the actual scope.
3. Verify any interfaces the architecture doc names (e.g., `POST /steer`, `x-usezombie.context.tool_window`) match what shipped.
4. If a spec was renamed or merged with another during implementation, update the architecture doc accordingly.

Output: a one-line note per spec in §14 — either "matches plan" or "deviated: <one line>".

**§4.2 Cold-read smoke test.** Pick one engineer (or fresh-context LLM in absence of one) who did not work on M40-M49. Have them read architecture/ end-to-end. Capture every place they pause to ask "what does this mean?" or "is this still true?". Fix those without diluting the doc — usually a one-sentence clarification in the offending paragraph.

**§4.3 New §14 "Ship Reflection" appendix.** Add a section at the end of architecture/ (after §13 Path to Bastion):

```markdown
## 14. Ship Reflection (post-launch, Q2 2026)

### What shipped vs planned
[1-3 paragraphs. Did the wedge ship as designed (GH Actions trigger + chat steer + Slack post)? Did the substrate (M40-M45) hold up? Did context layering (M41) avoid the embarrassment Codex predicted?]

### What surprised us
[1-2 paragraphs. Decisions that didn't survive contact with implementation. Operational learnings.]

### What we deferred
[1 paragraph. BYOK/M48 scope coverage. M47 approval inbox status. Self-host (still v3?). Install-skill host coverage.]

### Evidence
- Launch date: <YYYY-MM-DD>
- First external install: <YYYY-MM-DD>, <operator> at <company>
- Public artifacts: <URLs to launch post, HN thread, screen recording>
- First real external incident the zombie diagnosed: <YYYY-MM-DD, brief>
```

> **Implementation default:** §14 stays under 600 words. Reflection is what's NEW post-ship — surprises, deferred items, evidence. NOT a roadmap; future work goes in pending specs.

**§4.4 Numbering and anchor sanity.** Run `grep -E "§[0-9]+|\\[.*\\]\\(#[a-z-]+\\)"` on the doc; verify every section reference and anchor link resolves. Fix orphans.

**§4.5 README hero sync.** Update `README.md` (this repo) and `~/Projects/.github/README.md` (org-level GitHub profile) hero line to match architecture §0 differentiation pillars. Keep both byte-identical for the hero paragraph so they stay in sync.

**§4.6 Launch-tweet copy freeze.** 48h before ship date, freeze: tweet copy + landing-page hero + first-screenshot. Review against architecture §0 differentiation pillars. If any artifact drifts (e.g., still claims "self-hostable"), fix before ship — not after.

### §5 — `/privacy/cli-telemetry`

Privacy contract for the pingback. Mirror Turso / Resend tone — direct, no legalese. Cover:

- What we collect: anonymous install ID (random UUID, stored locally), skill version, timestamp, OS family (`darwin`, `linux`, `windows-wsl`, etc.).
- What we don't: repo names, file paths, email, IP (we discard after aggregation), Slack channel, credential identifiers.
- How to opt out: `gstack-config set usezombie_telemetry off` (or skill-equivalent flag — define in M49's prose).
- How to delete: tell us; we wipe by anonymous install ID.

### §6 — Install-pingback endpoint

`POST /v1/skills/install-pingback` (note: under `/v1/skills`, not `/v1/workspaces` — anonymous, no auth):

```
body:
  {
    install_id: <UUID generated locally on first install>,
    skill: "usezombie-install-platform-ops",
    skill_version: "0.1.0",
    os: "darwin"|"linux"|"windows-wsl"|"unknown",
    ts: <RFC3339>,
  }
→ 204 No Content
→ 400 if body schema invalid
```

Server: insert into `core.install_pingbacks` table (date-bucketed, no PII columns). IP discarded after `INSERT`.

> **Implementation default:** rate-limit to 1 request per `install_id` per day (idempotent). Multiple installs from the same anonymous user count as 1 unique install per day.

### §7 — Aggregation + dashboard

`src/state/install_metrics.zig`: read-only queries over `core.install_pingbacks`. Aggregations:

- Daily unique installs (count distinct `install_id` per UTC day)
- Per-skill installs
- OS distribution

Surface in the existing internal admin dashboard (or a new `/admin/installs` page). Not customer-facing.

### §8 — Cross-repo PR coordination

Docs repo changes are in `~/Projects/docs/`. Main repo changes are in `usezombie/usezombie`. Coordinate:

1. Build out the docs site changes on a branch in `~/Projects/docs/`. Preview deploy.
2. Build the pingback endpoint in the main repo on a branch.
3. Land the main-repo PR first (endpoint live, accepting traffic).
4. Land the docs-repo PR second (links to the now-live endpoint behavior).
5. Tag a release; announce.

---

## Interfaces

```
HTTP:
  POST /v1/skills/install-pingback (anonymous, no auth)
    body: { install_id: uuid, skill: string, skill_version: string, os: string, ts: rfc3339 }
    → 204 No Content
    → 400 on invalid schema
    rate limit: 1 req per install_id per day (silent dedupe)

DB:
  core.install_pingbacks (id, install_id, skill, skill_version, os, day_utc, created_at)
  unique (install_id, skill, day_utc)

Internal queries (admin-only):
  install_metrics.dailyUniques(skill?) → [{day, count}]
  install_metrics.osDistribution(skill?, since?) → [{os, count}]
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Pingback POST blocked by user firewall | Air-gapped env | Skill's pingback call has 5s timeout; on fail, silently continues (telemetry is best-effort) |
| Schema-invalid POST | Skill bug or attacker | 400; no insert |
| Duplicate `install_id` for same skill+day | Re-running skill | Silent dedupe (UNIQUE constraint); endpoint returns 204 either way |
| Docs page 404 from launch tweet link | Page not deployed yet | Verify: launch only proceeds after `curl https://docs.usezombie.com/quickstart/platform-ops` returns 200 |

---

## Invariants

1. **No PII in pingbacks.** Schema rejects anything beyond `install_id, skill, skill_version, os, ts`.
2. **Pingback is best-effort.** Skill never fails an install because the pingback failed.
3. **Anonymity preserved.** `install_id` is random; not derivable from environment.
4. **Privacy doc matches reality.** What we say we collect is what we collect — verified by reading the code.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_pingback_happy_path` | POST valid body → 204; row in `core.install_pingbacks` |
| `test_pingback_invalid_body` | POST with extra fields like `email` → 400; no insert |
| `test_pingback_dedupe_same_day` | Same install_id+skill+day twice → 204 both times; 1 row in DB |
| `test_pingback_no_pii_in_logs` | Mock logger; POST → grep logs for the install_id and skill_version → only intended fields appear (no IP/headers logged) |
| `test_quickstart_page_renders` | Build docs site → assert /quickstart/platform-ops/index.html exists with non-empty body |
| `test_concepts_context_lifecycle_renders` | Same as above for /concepts/context-lifecycle |
| `test_privacy_cli_telemetry_renders` | Same as above for /privacy/cli-telemetry |
| `test_no_self_host_page_in_v2` | Build docs site → assert /self-host returns 404 (page intentionally absent in v2; self-host is v3) |
| `test_homelab_pages_404` | After delete, /integrations/lead-collector and /launch/homelab-zombie return 404 |
| `test_admin_aggregation` | Insert 100 rows → query daily uniques → matches expected count |
| `test_arch_M_references_resolve` (folded from M50) | grep + ls — every `M{N}` mentioned in `docs/architecture/` has a corresponding `docs/v2/done/M{N}_*.md` file |
| `test_arch_anchor_links_resolve` (folded from M50) | grep `(#anchor)` style links in architecture/ → all targets exist as headers |
| `test_arch_section_14_present` (folded from M50) | After ship, `## 14. Ship Reflection` exists in architecture/ with non-empty content under 600 words |
| `test_arch_no_orphan_TODO` (folded from M50) | grep `TODO\|TKTK\|FIXME` in architecture/ → 0 hits |
| `test_readme_hero_sync` | Hero paragraph in `README.md` (this repo) is byte-identical to hero paragraph in `~/Projects/.github/README.md` |

---

## Acceptance Criteria

- [ ] All 15 tests pass (10 site/pingback + 5 architecture cross-reference, folded from M50)
- [ ] `docs.usezombie.com` deploys cleanly with the 4 new v2 pages live (quickstart, skills, concepts/context-lifecycle, privacy/cli-telemetry)
- [ ] `/self-host` returns 404 — no v2 stub for the v3 feature
- [ ] Hero copy reflects new positioning (3 pillars: OSS + BYOK + markdown-defined); old homelab references gone
- [ ] `POST /v1/skills/install-pingback` accepting traffic in production
- [ ] Privacy doc reviewed; matches what's actually collected (audit)
- [ ] Internal `/admin/installs` dashboard shows the first install (Customer Zero) on Day 0
- [ ] Manual: Customer Zero re-runs the install skill on a fresh laptop → pingback fires → row in DB → opt-out flag works
- [ ] **Architecture cross-reference pass complete** (folded from M50): every `M{N}` reference in `docs/architecture/` verified against `docs/v2/done/`
- [ ] **§14 Ship Reflection added** with real evidence (launch date, first external install, URLs, first real diagnosis)
- [ ] **Cold-read smoke test done** on `docs/architecture/`; resulting clarity fixes applied
- [ ] **No orphan TODOs / FIXMEs** in `docs/architecture/`
- [ ] **README hero synced**: `README.md` (this repo) and `~/Projects/.github/README.md` (org GitHub profile) carry the same hero paragraph
- [ ] **Launch-tweet copy frozen 48h pre-ship**: tweet copy + landing-page hero + first-screenshot reviewed against architecture §0 differentiation pillars; sign-off captured in Ripley's Log

---

## Out of Scope

- User-facing analytics / install metrics dashboard (admin-only in v1)
- Email or Slack notifications on install events
- Cohort / funnel analysis (just raw counts in v1)
- Geographic / IP-based segmentation (we don't retain IPs)
- Cross-skill metrics (each skill pings separately; cross-skill rollups are future work)
- **Self-host runbook page** — moved to v3. The `/self-host` URL intentionally 404s in v2; no "coming soon" stub.
- **Re-architecting based on post-launch learnings.** If substrate decisions look wrong in retrospect, file follow-up specs in `docs/v3/pending/`; do not rewrite architecture/ mid-cross-reference.
- **Marketing-tone polish on architecture/.** The architecture doc stays technical; marketing copy lives on docs.usezombie.com.

---

## Note on M50 fold (Apr 25, 2026)

M50_001 was originally a separate spec for "architecture/ cross-reference + post-launch reflection." Folded into M51 because:

1. **Same workstream owner.** Documentation drift (M50) and docs.usezombie.com positioning (M51) are both docs hygiene.
2. **No independent user value.** M50 was meta-work — internal team correctness, not operator-facing capability.
3. **Saves a milestone slot.** 12 → 11 specs reduces tier-tracking overhead without losing content.

What was M50 §1-§5 is now M51 §4.1-§4.6 (with the README sync + tweet-freeze deliverables added). All M50 acceptance criteria, tests, and invariants are absorbed above.
