# M51_001: docs.usezombie.com Positioning Rewrite + Install-Pingback Endpoint

**Prototype:** v2.0.0
**Milestone:** M51
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — packaging-blocking. The launch tweet links to `docs.usezombie.com/quickstart/platform-ops`; if it 404s or shows stale homelab-zombie content, the launch lands flat. The install-pingback is what gives us the Day-N install metric without DM-polling Twitter.
**Categories:** DOCS, API
**Batch:** B3 — depends on M40-M49 substrate + skill being shippable. Parallel with M50.
**Branch:** feat/m51-docs-and-pingback (to be created)
**Depends on:** M40-M46 (substrate that the docs describe), M49 (the install-skill being public). Nothing structural for the pingback endpoint itself.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §0, §3 (positioning) + §11 (context lifecycle — needs a user-facing doc).

---

## Implementing agent — read these first

1. `~/Projects/docs/` — the docs.usezombie.com source repo. Read its existing structure (likely Mintlify or similar): `mint.json`, navigation tree, hero copy, existing pages.
2. `docs/ARCHITECHTURE.md` (this repo) — the canonical reference; the docs site is the user-facing version of relevant sections.
3. M49's spec (sibling) for the install-skill flow — `/quickstart/platform-ops` walks through this.
4. Vercel / Cloudflare project configs (whichever hosts docs.usezombie.com today) — know how the site deploys.
5. Existing privacy doc patterns — Turso, Resend, PlanetScale all have CLI telemetry privacy pages worth mirroring for tone.

---

## Overview

**Goal (testable):** Operator visits `https://docs.usezombie.com` and sees:

1. **Hero**: *"Durable, self-hostable, BYOK, markdown-defined agent runtime — for operators who run their own infra."* — replaces any "AI for SREs" framing. Free hosted zombid; OSS code; self-host when ready.
2. **`/quickstart/platform-ops`** — single page walking through `/install-platform-ops` skill from agent installation through first Slack post. Includes screenshots / Loom embed.
3. **`/skills`** — describes the skill catalog (`install-platform-ops` for now; future skills) and how to drop into `~/.claude/skills/` or fetch via `usezombie.sh`.
4. **`/concepts/context-lifecycle`** — user-facing version of §11 in ARCHITECHTURE.md. Includes the L1+L2+L3 ASCII diagram and the override table.
5. **`/self-host`** — runbook with the per-row tested-as-of badges (the 6-row table from the design doc: Auth, Postgres, Redis, Process orchestration, Executor sandbox, Secrets).
6. **`/privacy/cli-telemetry`** — privacy contract for the install-pingback (what we collect, what we don't, opt-out flag).

Plus: `POST https://api.usezombie.com/v1/skills/install-pingback` is live, accepting anonymous install events from the install-skill (M49). Returns 204 No Content. Daily aggregates visible in an internal dashboard.

**Problem:** The current docs site (`~/Projects/docs/`) still talks about homelab-zombie and a kubectl-first narrative that no longer ships. If the launch tweet links to it, readers see a contradiction with the tweet's claim. There's also no install metric — Day-21 / Day-50 validation depends on knowing if anyone installed.

**Solution summary:** Two parallel deliverables. Docs site rewrite (positioning + 6 new pages, deprecate stale ones). Plus a thin server-side install-pingback endpoint that the install-skill POSTs to anonymously after a successful install. Privacy-first: anonymized install ID (random UUID stored locally, not user/repo identity), skill version, timestamp, OS family. No repo names, no email, no token, no IP retention beyond aggregation.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/index.mdx` (or equivalent) | EDIT | Hero copy rewrite |
| `~/Projects/docs/quickstart/platform-ops.mdx` | NEW | Install + first-chat walkthrough |
| `~/Projects/docs/skills/index.mdx` | NEW | Skill catalog overview |
| `~/Projects/docs/skills/install-platform-ops.mdx` | NEW | Detail page for the install skill |
| `~/Projects/docs/concepts/context-lifecycle.mdx` | NEW | User-facing context layering doc |
| `~/Projects/docs/self-host.mdx` | NEW | Self-host runbook with tested-as-of table |
| `~/Projects/docs/privacy/cli-telemetry.mdx` | NEW | Pingback privacy contract |
| `~/Projects/docs/mint.json` (or nav config) | EDIT | Add new pages to nav |
| `~/Projects/docs/integrations/lead-collector.mdx` | DELETE if exists | Stale homelab-era content |
| `~/Projects/docs/launch/homelab-zombie.mdx` | DELETE if exists | Same |
| `src/http/handlers/skills/install_pingback.zig` | NEW | The pingback endpoint |
| `src/http/router.zig` | EXTEND | Wire `/v1/skills/install-pingback` |
| `src/state/install_metrics.zig` | NEW | Aggregation: count by day, skill version, OS family |
| `tests/integration/install_pingback_test.zig` | NEW | E2E: POST → 204; aggregation increments |

> **Cross-repo PR**: docs site changes are in a different repo (`~/Projects/docs/`). Coordinate the merge timing with the main repo's launch.

---

## Sections (implementation slices)

### §1 — Hero copy + nav rewrite

Hero copy on landing page leads with: *"Durable, self-hostable, BYOK, markdown-defined agent runtime — for operators who run their own infra."* Sub-line: *"Free hosted. Open source. Self-host when you're ready."* Below the fold: 30-second install demo (Loom embed or animated GIF) of `/install-platform-ops` running.

> **Implementation default:** mirror the visual rhythm of Resend.com or Turso.com docs — punchy hero, tight code snippet, three-card differentiation block, then the quickstart link. No marketing fluff.

Navigation: top-level entries reorganized to:
- Quickstart
- Concepts (incl. context lifecycle)
- Skills
- API Reference
- Self-Host
- Privacy

### §2 — `/quickstart/platform-ops`

Single page, top-to-bottom walkthrough:

1. Prerequisite: Claude Code (or Amp, Codex CLI, OpenCode) installed locally. `zombiectl` installed (link to install).
2. Run `zombiectl auth login` (signs in via Clerk OAuth).
3. Run `/install-platform-ops` in Claude Code.
4. Answer the 4 prompts (Slack channel, branch glob, cron opt-in, BYOK optional).
5. Skill installs the zombie + posts a first response to Slack.
6. Set up the GH webhook: copy the URL + secret the skill emits, paste into the GH repo's webhook settings.
7. Trigger: cause a deploy failure → see the Slack diagnosis arrive.

Include real screenshots (or Loom). Author's repo as the demo target.

### §3 — `/concepts/context-lifecycle`

User-facing version of §11 in ARCHITECHTURE.md. Same ASCII diagram. Same override table. Add: a "common questions" section addressing things like "do I need to tune these?" (answer: no, defaults work), "what if my zombie needs more depth?" (answer: bump `tool_window` first; everything else is fine for 95% of cases).

### §4 — `/self-host`

Runbook with the per-row tested-as-of table:

| Component | Substitution path | Status (as of YYYY-MM-DD) |
|-----------|-------------------|---------------------------|
| Auth | Clerk → env-token / OIDC shim | ⚠ Untested — runbook only |
| Postgres | PlanetScale → standard Postgres URL | ✓ Tested |
| Redis | Upstash → standard Redis URL | ✓ Tested |
| Process orchestration | Fly machines → Linux VM (api/worker/executor as systemd or compose) | ⚠ Untested — runbook only |
| Executor sandbox | Landlock + cgroups + bwrap on Fly Linux → same on vanilla Ubuntu 24.04 | ⚠ Untested — runbook only |
| Secrets / KMS | Fly KMS envelope → portable adapter | ⚠ Untested — runbook only |

Untested rows link to follow-up GitHub issues for tracking validation. The launch claim is "self-hostable in principle, with these caveats" — honest.

> **Implementation default:** as M{N+}_001 self-host validation specs (filed per A3 in the design doc) ship, update the badges to ✓ with the date.

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
    skill: "install-platform-ops",
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
| `test_self_host_runbook_renders` | Same as above for /self-host |
| `test_homelab_pages_404` | After delete, /integrations/lead-collector and /launch/homelab-zombie return 404 |
| `test_admin_aggregation` | Insert 100 rows → query daily uniques → matches expected count |

---

## Acceptance Criteria

- [ ] All 10 tests pass
- [ ] `docs.usezombie.com` deploys cleanly with new pages live
- [ ] Hero copy reflects new positioning; old homelab references gone
- [ ] `POST /v1/skills/install-pingback` accepting traffic in production
- [ ] Privacy doc reviewed; matches what's actually collected (audit)
- [ ] Internal `/admin/installs` dashboard shows the first install (Customer Zero) on Day 0
- [ ] Manual: Customer Zero re-runs the install skill on a fresh laptop → pingback fires → row in DB → opt-out flag works

---

## Out of Scope

- User-facing analytics / install metrics dashboard (admin-only in v1)
- Email or Slack notifications on install events
- Cohort / funnel analysis (just raw counts in v1)
- Geographic / IP-based segmentation (we don't retain IPs)
- Cross-skill metrics (each skill pings separately; cross-skill rollups are future work)
