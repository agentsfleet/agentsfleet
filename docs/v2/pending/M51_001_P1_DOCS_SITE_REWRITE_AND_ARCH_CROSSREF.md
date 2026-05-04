# M51_001: docs.usezombie.com Positioning Rewrite + Architecture Cross-Reference

**Prototype:** v2.0.0
**Milestone:** M51
**Workstream:** 001
**Date:** Apr 25, 2026 · revised May 03, 2026
**Status:** PENDING
**Priority:** P1 — packaging-blocking. The launch tweet links to `docs.usezombie.com/quickstart/platform-ops`; if it 404s or shows stale homelab-zombie content, the launch lands flat.
**Categories:** DOCS
**Batch:** B3 — depends on all other v2 substrate + packaging being shippable. Final milestone before launch.
**Branch:** TBD
**Depends on:** M40-M49 (substrate + packaging — cross-reference pass walks every shipped spec).
**Folded in:** M50 (architecture cross-reference + post-launch reflection) — formerly a separate spec, merged here Apr 25, 2026 because the docs workstream owns documentation drift, M50 was meta-work without independent user value, and consolidating reduces milestone count.
**Scope reduction (May 03, 2026):** install-pingback endpoint and `/privacy/cli-telemetry` page removed from scope — Day-N adoption is measured by other means (npm download counts, Customer Zero anecdotes, GitHub issue volume, organic Slack DMs). Building anonymous-telemetry plumbing for a dataset we don't yet need is heavier than the answer it would give. See "Out of Scope" for the deferral rationale; if Day-N data becomes load-bearing, file a fresh spec then.

**Canonical architecture:** `docs/architecture/` §0, §3 (positioning) + §11 (context lifecycle — needs a user-facing doc). This spec also keeps `docs/architecture/` itself accurate post-ship (formerly M50's job).

---

## Cross-spec amendment (Apr 30, 2026 — folded from M43 review pass)

The `/quickstart/platform-ops` walkthrough touches surfaces that the M43 webhook review pinned. Two reinforcements:

**D1 — Quickstart step 6 wording.** The current draft (§2 step 6: "Set up the GH webhook: copy the URL + secret the skill emits, paste into the GH repo's webhook settings.") matches the post-M43 design. Concretize the URL: the user pastes `https://api.usezombie.com/v1/webhooks/{zombie_id}`. The secret is the value the install-skill (M49) generated and showed once during install — not stored anywhere user-visible after that moment. The doc explicitly says: "Lost the secret? Rotate the workspace `github` credential with `zombiectl credential add github --data @-` and pipe the JSON on stdin."

**D2 — Workspace-scoped webhook credential.** The quickstart must show that one operator at one workspace pastes the same secret into N repo webhook configs (one per zombie covering N repos). This is the actually-simple operator UX that the workspace-credential design unlocks; the doc should say so plainly. Tradeoff (also document): rotation is workspace-wide; rotating affects every zombie in the workspace.

No file additions or removals from §M51 §Files Changed table from this amendment.

---

## Cross-spec amendment (May 04, 2026 — marketing site rewrite folded in)

The marketing site at `ui/packages/website/` (usezombie.com — Vite + React + react-router, separate from the Mintlify docs site at `~/Projects/docs/`) was built for a v1 product framing — "AI-generated PRs", "automated PR delivery", "validation before review", "Connect GitHub, automate PRs", "babysit the run". That positioning no longer matches what shipped under M40-M49. The launch-tweet hero (§4.6) lives on this site; on launch morning, traffic from the tweet hits usezombie.com first and bounces to docs.usezombie.com second. Both surfaces must carry the same three pillars (open source + BYOK + markdown-defined) and the same wedge (platform-ops deploy-failure responder + manual steer). If they don't, the contradiction is visible inside five seconds of a tweet click.

This amendment folds the marketing-site rewrite into M51 as `§6` below. Animation, layout, design-system components, motion library, lazy chunks, the animated terminal, BackgroundBeamsWithCollision, the SVG zombie mark, PostHog analytics events, `data-testid` attributes, and route-fade transitions all stay untouched. Only string literals, the install-command snippet, and the Hero primary-CTA destination change.

**Order of operations (Captain's directive May 04, 2026):**

1. Marketing site rewrite (`ui/packages/website/`) lands first.
2. Architecture cross-reference + §14 ship reflection (this repo's `docs/architecture/`) — §4.1-§4.6.
3. Docs site rewrite (`~/Projects/docs/`) — §1-§3.

This sequencing is load-bearing for the launch tweet: the marketing site is the most-visited surface on launch day, the docs site is the second-most, and the architecture doc is the internal source of truth both marketing and docs reference. Landing in this order means a tweet-clicker never sees the contradiction window.

**Hero primary-CTA destination (decided May 04, 2026):** the Hero primary CTA moves from `APP_BASE_URL` (Mission Control sign-up) to `DOCS_QUICKSTART_URL` (the platform-ops walkthrough). Sign-up happens during `zombiectl auth login` inside the install skill — sending users to a sign-up form before they install the runtime is backwards. PostHog event `signup_started` continues to fire from the secondary CTA and the InstallBlock.

---

## Implementing agent — read these first

1. `~/Projects/docs/` — the docs.usezombie.com source repo. Read its existing structure (likely Mintlify or similar): `mint.json`, navigation tree, hero copy, existing pages.
2. `docs/architecture/` (this repo) — the canonical reference; the docs site is the user-facing version of relevant sections.
3. M49's spec (sibling) for the install-skill flow — `/quickstart/platform-ops` walks through this.
4. Existing developer-tool docs from comparable players (Turso, Resend, PlanetScale, `gstack`) for tone, hero rhythm, and quickstart structure. Mirror the best parts of their visual cadence, not their feature breadth.

---

## Overview

**Goal (testable):** Operator visits `https://docs.usezombie.com` and sees:

1. **Hero**: *"Durable, BYOK, markdown-defined agent runtime — for operators who own their outcomes."* — replaces any "AI for SREs" framing. Three differentiation pillars: OSS + BYOK + markdown-defined. Free hosted; open source; **self-host arrives in v3**.
2. **`/quickstart`** (rewritten in place — May 04, 2026 decision) — single page walking through `/usezombie-install-platform-ops` from agent installation through first Slack post. The existing v1 quickstart content is overwritten; the URL stays stable so cached / inbound links keep resolving. No `/quickstart/platform-ops` subpath until a second flagship zombie exists. Includes screenshots and a short screen recording.
3. **`/skills`** — describes the `usezombie-*` skill family (`usezombie-install-platform-ops` for now; future `usezombie-steer`, `usezombie-doctor`). Three install paths documented: (a) `npm install -g @usezombie/zombiectl` (one-time CLI install), (b) host-agent skill install via `/usezombie-install-platform-ops` (typical user — runs inside Claude Code / Amp / Codex CLI / OpenCode and drives `zombiectl install --from` under the hood), (c) `zombiectl install --from ~/.config/usezombie/samples/platform-ops` (power-user / scripted install from the local sample directory shipped with `zombiectl`).
4. **`/concepts/context-lifecycle`** — user-facing version of §11 in architecture/. Includes the L1+L2+L3 ASCII diagram and the override table.
5. **`usezombie.com` (marketing site at `ui/packages/website/`)** — Hero, Home features, HowItWorks, FeatureFlow, FAQ, ProviderStrip, CTABlock, Agents, Pricing, Footer all carry the same three pillars and platform-ops wedge. Copy-only rewrite; animation, layout, and design-system component scaffolding untouched. See §6.

**Plus (folded from M50):** `docs/architecture/` cross-referenced against shipped specs and updated with a §14 ship reflection. Every `(M{N})` mention in the architecture doc points at a real spec in `docs/v2/done/`. §14 captures what shipped vs planned, what surprised us, what was deferred.

**Launch-tweet copy freeze:** 48h before ship date, the launch tweet copy + landing-page hero + first-screenshot are signed off against the architecture doc's §0 differentiation pillars. Catches the moment the tweet drifts from the substrate truth (e.g., accidentally claims self-host).

**Problem:** The current docs site (`~/Projects/docs/`) still talks about homelab-zombie and a kubectl-first narrative that no longer ships. If the launch tweet links to it, readers see a contradiction with the tweet's claim. Separately, the architecture doc was rewritten BEFORE substrate shipped; predictions in it (e.g., "M43 owns webhook ingest") are guesses until reconciled.

**Solution summary:** Two parallel deliverables. (1) Docs site rewrite — positioning + 3 new pages, deprecate stale ones. (2) Architecture cross-reference + §14 ship reflection — keeps the canonical doc honest post-ship. README hero stays in sync with architecture §0. Launch tweet copy frozen 48h pre-ship against the same source of truth.

---

## Files Changed (blast radius)

### Docs site (`~/Projects/docs/`)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/index.mdx` | EDIT | Hero copy rewrite to three v2 pillars |
| `~/Projects/docs/quickstart.mdx` | EDIT (overwrite) | v1 content (157 lines) replaced with platform-ops walkthrough; URL stays `/quickstart` per Q-new C |
| `~/Projects/docs/concepts.mdx` | EDIT (rewrite) | Audit + replace v1 framing (Q1) |
| `~/Projects/docs/how-it-works.mdx` | DELETE | v1-only page; job covered by new quickstart + concepts/context-lifecycle (Q1) |
| `~/Projects/docs/skills/index.mdx` | NEW | Skill catalog overview |
| `~/Projects/docs/skills/usezombie-install-platform-ops.mdx` | NEW | Detail page for the install skill |
| `~/Projects/docs/concepts/context-lifecycle.mdx` | NEW | User-facing context layering doc |
| `~/Projects/docs/docs.json` (Mintlify renamed `mint.json`) | EDIT | Add new pages to nav; remove `Self-hosting` group (16 pages); prune dead API routes (Q3); **Do NOT add a Self-Host nav entry** — self-host is v3 |
| `~/Projects/docs/.mintignore` | EDIT | Add `operator/**` so the 16 self-host pages don't render; files stay on disk for v3 revival (Q2) |
| `~/Projects/docs/operator/**` | PRESERVE | Files stay on disk, hidden by `.mintignore`; `git rm` the ignore line when v3 self-host ships |
| `~/Projects/docs/integrations/lead-collector.mdx` | DELETE if exists | Stale homelab-era content |
| `~/Projects/docs/launch/homelab-zombie.mdx` | DELETE if exists | Same |
| `~/Projects/docs/self-host.mdx` | DO NOT CREATE | Self-host deferred to v3; no v2 page |

### Marketing site (`ui/packages/website/` — usezombie.com)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/components/Hero.tsx` | EDIT | Hero copy + install command + primary CTA destination (copy-only; animation untouched) |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | 6 feature cards rewritten to v2 capabilities; section H2 reframed |
| `ui/packages/website/src/components/HowItWorks.tsx` | EDIT | 3-step flow rewritten: trigger → evidence → diagnosis |
| `ui/packages/website/src/components/FeatureFlow.tsx` | EDIT | 3 animated rows: copy + bullets per row; install panel command updated; mission grid relabelled |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | 6 Q/A rewritten; replaces v1 PR-validator framing |
| `ui/packages/website/src/components/ProviderStrip.tsx` | EDIT | Repurposed from "GitHub/CLI/API" surfaces to BYOK provider list |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | Agent-page CTA realigned to OpenAPI 3.1 surface framing |
| `ui/packages/website/src/components/Footer.tsx` | EDIT | Brand tagline single-line edit |
| `ui/packages/website/src/pages/Agents.tsx` | EDIT | DEMO_COMMANDS + DEMO_OUTPUTS rewritten to platform-ops; apiOps table updated; webhook payload updated; InstallBlock command + Bootstrap pre-block updated |
| `ui/packages/website/src/pages/Pricing.tsx` | EDIT | Tier highlights, lead, roadmapSignals rewritten to BYOK + hosted-execution framing |
| `ui/packages/website/src/components/HeroIllustration.tsx` | UNCHANGED | Pure SVG zombie mark; no copy |
| `ui/packages/website/src/config.ts` | UNCHANGED | `DOCS_QUICKSTART_URL` stays at `/quickstart` per Q-new C (collapse) |
| `ui/packages/website/tests/e2e/*.spec.ts` | EDIT | Smoke spec string assertions updated to match new copy |

### Cross-cutting

| File | Action | Why |
|------|--------|-----|
| `README.md` (root, this repo) | EDIT | Hero line synced to architecture §0 differentiation pillars |
| `~/Projects/.github/README.md` (org-level GitHub profile) | EDIT | Same hero line, public-facing |
| `docs/architecture/` | EDIT | Cross-reference correctness pass + §14 ship reflection (folded from M50) |

> **Cross-repo coordination**: this milestone touches three repos — `usezombie/usezombie` (this one — marketing site under `ui/packages/website/`, architecture under `docs/architecture/`, root README), `~/Projects/docs/` (Mintlify docs site), and `~/Projects/.github/` (org GitHub profile). Land in the order: marketing site → architecture → docs site → READMEs (per Captain's directive May 04, 2026).

---

## Sections (implementation slices)

### §1 — Hero copy + nav rewrite

Hero copy on landing page leads with: *"Durable, BYOK, markdown-defined agent runtime — for operators who own their outcomes."* Sub-line: *"Free hosted. Open source. Self-host arrives in v3."* Below the fold: 30-second install demo (short screen recording or animated GIF) of `/usezombie-install-platform-ops` running.

> **Implementation default:** mirror the visual rhythm of Resend.com or Turso.com docs — punchy hero, tight code snippet, three-card differentiation block, then the quickstart link. No marketing fluff.

Navigation: top-level entries reorganized to:
- Quickstart (rewritten in place; same `/quickstart` URL)
- Concepts (incl. context lifecycle)
- Skills
- API Reference (with dead routes pruned per Q3)

The `Self-hosting` group (16 `operator/*` pages) is removed from `docs.json` navigation. Files stay on disk under `~/Projects/docs/operator/`, hidden by `.mintignore` (Q2 decision May 04, 2026). When v3 self-host ships, a fresh spec removes the `.mintignore` line and re-adds the nav group.

(No `Self-Host` nav entry — self-host is v3. The `/self-host` URL intentionally 404s in v2; see Out of Scope and `test_no_self_host_page_in_v2`. Every `/operator/*` URL also 404s. No `Privacy` entry either — no telemetry collected, no privacy doc needed.)

**API tab pruning (Q3 — May 04, 2026).** Cross-check `docs.json` API tab routes against `~/Projects/usezombie/public/openapi.json`. Drop any group entry that doesn't exist in shipped OpenAPI. Confirmed candidates from initial audit:
- `POST /v1/execute` — removed under M10 (pipeline v1 removal); drop from `Execution` group
- `GET /v1/internal/v1/telemetry` — internal-only; drop from `Internal` group (or move group out of public docs entirely)

The full prune list is computed by the implementing agent; the spec asserts the test (`test_api_tab_routes_resolve_in_openapi`) catches drift in CI.

### §2 — `/quickstart` (rewritten in place)

Single page, top-to-bottom walkthrough. URL stays `/quickstart` (Q-new C); v1 content is overwritten.

1. Prerequisite: Claude Code (or Amp, Codex CLI, OpenCode) installed locally. Install `zombiectl` with `npm install -g @usezombie/zombiectl`.
2. Run `zombiectl auth login` (signs in via Clerk OAuth).
3. Run `/usezombie-install-platform-ops` in any supported host (Claude Code, Amp, Codex CLI, OpenCode). The skill drives `zombiectl install --from ~/.config/usezombie/samples/platform-ops` under the hood; power users can run that directly.
4. Answer the 3 prompts (Slack channel, branch glob, cron opt-in). BYOK setup is a separate later step if you want to bring your own key — see the BYOK page.
5. Skill installs the zombie + posts a first response to Slack.
6. Set up the GH webhook: copy the URL + secret the skill emits, paste into the GH repo's webhook settings. The webhook URL is `https://api.usezombie.com/v1/webhooks/{zombie_id}`; the secret is shown once during install. Lost the secret? Rotate the workspace `github` credential with `zombiectl credential add github --data @-` and pipe the JSON on stdin (per Apr 30 amendment §D1).
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
[1 paragraph. BYOK/M48 scope coverage. M47 approval inbox status. Self-host (still v3?). Install-skill host coverage. Install pingback (deferred until Day-N data becomes load-bearing).]

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

### §5 — Cross-repo PR coordination

Three repos touched: this repo (`usezombie/usezombie` — marketing site under `ui/packages/website/`, architecture under `docs/architecture/`, root README), `~/Projects/docs/` (Mintlify docs site), `~/Projects/.github/` (org GitHub profile).

Land in this order (Captain's directive May 04, 2026):

1. **Marketing site rewrite** in this repo on its own branch (`feat/m51-marketing-rewrite`). Preview deploy verified before merge. PostHog event names unchanged.
2. **Architecture cross-reference pass + §14** in this repo on its own branch (or same branch as marketing if scope is small). Architecture must be in `done/` before docs work begins so docs cross-references resolve.
3. **Docs site rewrite** in `~/Projects/docs/`. Mintlify preview deploy verified before merge.
4. **README sync** — root README + org-profile README in lockstep.
5. Tag a release; announce.

### §6 — Marketing site rewrite (`ui/packages/website/`)

Copy-only rewrite. Every animation, lazy chunk, motion-library binding, design-system component, PostHog event name, `data-testid` attribute, and the SVG zombie mark stays put. The intent is to make the most-visited launch-day surface match the architecture's three pillars without a structural rewrite.

**Forbidden strings** (must not appear in `ui/packages/website/src/**/*.tsx` after this lands — `test_marketing_no_legacy_pr_framing` enforces):
- `AI-generated PRs`
- `Automated PR delivery`
- `babysit`
- `Connect GitHub, automate PRs`
- `Validated PR delivery`
- `Run quality scoring` / `run quality scoring`
- `Review a validated PR`
- `Queue work` / `queued engineering work`
- `Validation before review`
- `usezombie.sh/install.sh` (replaced with `npm install -g @usezombie/zombiectl` everywhere)

These map to the v1 PR-validator framing the product pivoted away from.

#### §6.1 `Hero.tsx`
- `badge`: `Durable agent runtime · BYOK · Open source`
- `line1`: `Operational outcomes`
- `line2`: `don't fall into limbo.`
- `kicker`: `UseZombie is a durable, markdown-defined agent runtime. The flagship platform-ops zombie wakes on a GitHub Actions deploy failure, gathers evidence from Fly, Upstash, and your run logs, and posts an evidenced diagnosis to Slack — and keeps reasoning after your terminal closes. Bring your own model key. Read every line of the runtime.`
- Primary CTA label: `Install platform-ops`
- Primary CTA href: `DOCS_QUICKSTART_URL` (was `APP_BASE_URL`; sign-up happens during `zombiectl auth login` inside the install skill)
- Secondary CTA label: `See pricing` (unchanged)
- Terminal command card label: `Quick start command` (unchanged)
- Terminal command body: `npm install -g @usezombie/zombiectl`
- Terminal note: `Then run /usezombie-install-platform-ops in Claude Code, Amp, Codex CLI, or OpenCode.`

#### §6.2 `Home.tsx`
Section H2: `A long-lived runtime that owns the outcome until it's resolved or blocked.`

`features` array (replace all 6 entries):
- `01 Markdown-defined behaviour` — SKILL.md + TRIGGER.md. Iterate on prose, not redeploys.
- `02 Three triggers, one loop` — Webhook (GitHub Actions), cron, and `zombiectl steer` all flow through the same reasoning. The zombie doesn't branch on actor type.
- `03 Bring Your Own Key` — Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot. The executor treats your provider key as another secret resolved at the tool bridge.
- `04 Reasons past the context limit` — Memory checkpoints, rolling tool-result window, and stage chunking compose so deep incidents continue past the model's working-memory cap.
- `05 Approval gating` — Risky actions block until a human clicks Approve in the dashboard or Slack. State machine survives worker restarts.
- `06 Open-source runtime` — The code that holds your credentials and runs against your infrastructure is code you can read.

`InstallBlock` props:
- `title`: `Install zombiectl, then run /usezombie-install-platform-ops`
- `command`: `npm install -g @usezombie/zombiectl`
- `actions`: keep the two-button shape; right button label → `Install platform-ops` → `DOCS_QUICKSTART_URL`; left button label → `Read the docs` → `DOCS_URL`.

#### §6.3 `HowItWorks.tsx`
H2: `From trigger to evidenced diagnosis, durably.`
Eyebrow: `How it works`

3 steps:
- `A trigger arrives` — A GitHub Actions deploy fails, a cron fires, or you run `zombiectl steer`. Each lands on the event stream with actor provenance: `webhook:github`, `cron:<schedule>`, `steer:<user>`.
- `The zombie gathers evidence` — It calls the tools `TRIGGER.md` allow-lists — `http_request`, `memory_store`, `cron_add`. Secrets substitute at the sandbox boundary; the model sees placeholders, never raw bytes.
- `Diagnosis posts; the run is auditable` — Slack receives the evidenced diagnosis. Every event is on `core.zombie_events` with actor and timestamp. `zombiectl steer {id}` picks the conversation up later.

#### §6.4 `FeatureFlow.tsx`
3 animated rows. Panel kinds (`install`, `trace`, `mission`) and the layout stay; only copy + the install-panel command change. The `trace` panel already shows `event: workflow_run.failed` → `slack: #platform-ops` and is on-message.

`install` row:
- `title`: `Install once. Operate forever.`
- `description`: `One command installs zombiectl, one skill installs the platform-ops zombie. The skill detects your repo shape, asks three gating questions, and writes .usezombie/platform-ops/SKILL.md + TRIGGER.md.`
- `bullets`: `Host-neutral skill: Claude Code, Amp, Codex CLI, OpenCode` / `Detects fly.toml, GitHub Actions workflows, monorepo layouts` / `Idempotent re-runs against the same workspace`
- `ctaLabel`: `Install guide`
- `ctaHref`: `DOCS_QUICKSTART_URL`
- Install panel `<p className="feature-flow-code">` body: `$ npm install -g @usezombie/zombiectl`

`trace` row:
- `title`: `Every event, every actor, on the record.`
- `description`: `Every steer, webhook, and cron fire lands on zombie:{id}:events with actor provenance. Replay the full timeline. Stream live via SSE. Audit who or what triggered each step.`
- `bullets`: `Append-only event stream with actor=webhook|cron|steer|continuation` / `SSE tail at /v1/.../events/stream` / `Stage chunking preserves long-running reasoning across context boundaries`
- `ctaLabel`: `Read docs`
- `ctaHref`: `DOCS_URL`

`mission` row:
- `title`: `Mission Control`
- `description`: `Approvals, budgets, BYOK provider switching, and the kill switch — one dashboard. Approve a risky action from a Slack DM or the web.`
- `bullets`: `Per-day and per-month dollar caps; trip-blocked at the gate` / `Switch BYOK provider with zombiectl tenant provider set --credential <name>` / `zombiectl kill checkpoints state; nothing lost`
- `ctaLabel`: `Open Mission Control` (unchanged)
- `ctaHref`: `APP_BASE_URL` (unchanged)
- Mission grid cell labels (replace existing AI Code / 78% / Runs / 243 / Merged / 91%): `Zombies` / `12` / `Approvals` / `3` / `Credits` / `$7.40`

#### §6.5 `FAQ.tsx`
Replace all 6 Q/A entries:

1. **What is UseZombie?** — A durable runtime for one operational outcome. v2 ships the platform-ops flagship: a zombie that wakes on a GitHub Actions deploy failure, gathers evidence from Fly / Upstash / Redis / GitHub run logs, and posts an evidenced diagnosis to Slack. The same zombie is reachable via `zombiectl steer` for manual investigation.

2. **What does BYOK mean?** — Bring Your Own Key. You store your own LLM provider credential — Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot — and the executor resolves it at the tool bridge. UseZombie marks up zero on inference. You pay your provider directly.

3. **What am I actually paying for?** — Hosted execution. Hosted runs are metered against a credit pool with a $10 starter grant that never expires; the two debit points are event receipt and per-stage execution. Inference cost is yours, paid directly to your model provider via BYOK.

4. **Can I self-host?** — Not in v2. v2 ships hosted-only on api.usezombie.com via Clerk OAuth. Self-host arrives in v3. The runtime is open source today; the auth substrate and KMS adapter are the only deployment-specific layers, so v3 self-host is mechanical, not architectural.

5. **Which agent hosts work for the install skill?** — Claude Code, Amp, Codex CLI, and OpenCode — same skill, same prompts in every host. `npm install -g @usezombie/zombiectl`, then run `/usezombie-install-platform-ops` inside any of them.

6. **What if my zombie hits the model's context window?** — It won't. The runtime layers three independent mechanisms — periodic memory checkpoints, a rolling tool-result window, and stage chunking — so a long incident keeps reasoning past the model's working-memory cap. Defaults work for 95% of cases. See concepts/context-lifecycle.

#### §6.6 `ProviderStrip.tsx`
Repurpose: `surfaces` array becomes BYOK provider list.
- `label` (the visible header): `Bring your own model`
- `surfaces` array: `["Anthropic", "OpenAI", "Fireworks · Kimi K2", "Together", "Groq", "Moonshot"]`

#### §6.7 `CTABlock.tsx`
- H2: `Building agents on UseZombie?`
- p: `Stable machine surface via OpenAPI 3.1. Webhook ingest, steer, event streams, approval grants — the same surfaces the human dashboard uses.`
- CTAs unchanged

#### §6.8 `Agents.tsx`
Keep `BackgroundBeamsWithCollision`, `AnimatedTerminal`, scanline, JSON-LD block, table layout. Surgical fixes:

- `DEMO_COMMANDS`: `["zombiectl auth login", "/usezombie-install-platform-ops", "zombiectl steer zmb_2041 \"morning health check\""]`
- `DEMO_OUTPUTS`: `{1: ["Generated .usezombie/platform-ops/SKILL.md + TRIGGER.md", "Installed platform-ops@0.1.0", "Webhook URL: https://api.usezombie.com/v1/webhooks/zmb_2041"], 2: ["[steer] gathering evidence: fly status, upstash health, last 3 runs…", "[steer] diagnosis posted to #platform-ops"]}`
- `InstallBlock` `title`: `Install Zombiectl` (unchanged)
- `InstallBlock` `command`: `npm install -g @usezombie/zombiectl`
- Bootstrap `<pre>` block:
  ```
  # Authenticate, install the platform-ops zombie
  npm install -g @usezombie/zombiectl
  zombiectl auth login
  /usezombie-install-platform-ops    # in Claude Code, Amp, Codex CLI, or OpenCode
  ```
- `apiOps` table: drop `Execute tool POST /v1/execute` (M10 removed). Add `Steer / chat — POST /v1/workspaces/:wid/zombies/:zid/messages — Send a steer message to a zombie.` Verify the rest against `public/openapi.json` during implementation.
- Webhook payload `webhookPayload` const: replace email.received placeholder with a minimal GitHub workflow_run.failed shape:
  ```json
  {
    "event_id": "evt_01JEXAMPLE",
    "action": "completed",
    "workflow_run": {
      "conclusion": "failure",
      "html_url": "https://github.com/usezombie/usezombie/actions/runs/123"
    }
  }
  ```
- H1: `This page is for autonomous agents.` (unchanged)
- Lead `<p>`: `Same product, machine-readable surface. Use /openapi.json as the canonical machine surface.` (light edit — removes the existing "Docs are secondary" line which reads wrong post-launch, and reframes from the existing v1 wording)
- Beams `<h2>`: `Zombie agents own outcomes; humans approve risky actions.` (was: "Zombie agents orchestrate work, humans approve.")
- Beams sub-`<p>`: `Install, run, and observe the zombie lifecycle without leaving this page.` (was: "agent lifecycle")
- `jsonLd.url`: leave as `https://usezombie.sh/agents` (`.sh` is the skill distribution domain per `plan_engg_review_v2.md` §Surfaces under test)

#### §6.9 `Pricing.tsx`
- `roadmapSignals`: `["BYOK with no token markup", "Open source runtime", "Three triggers, one reasoning loop", "Self-host arrives in v3"]`
- Hero H1: `Start free. Upgrade when you need stronger control.` (unchanged)
- Hero `lead`: `UseZombie sells durable execution and operational ownership — not marked-up model usage. Bring your own model key; pay your provider directly. Hosted execution is metered against a credit pool with a $10 starter grant.`

Hobby tier:
- `audience`: `For solo operators evaluating the wedge.`
- `proof`: `Best for installing platform-ops on a real repo and seeing a real diagnosis.`
- `highlights`: `["$10 starter credit, never expires", "1 workspace", "BYOK on Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot", "Hosted execution metered per stage; no token markup"]`

Scale tier:
- `audience`: `For teams running zombies across shared workspaces.`
- `proof`: `Built for teams operating multiple zombies across shared workspaces with approval gates and budget controls.`
- `highlights`: `["Everything in Hobby", "Multiple workspaces with shared event history", "Higher concurrency and longer per-stage windows", "Approval gating in dashboard and Slack DM", "Workspace-scoped credentials and webhooks", "Priority support"]`

Bottom band:
- eyebrow: `When to move up` (unchanged)
- H2: `Start on Hobby. Move to Scale when zombies become shared infrastructure.`
- p: `Hobby validates the wedge on one repo. Scale adds shared event history, approval flows, and the budget controls a team needs once zombies own real production outcomes.`

#### §6.10 `Footer.tsx`
- Brand tagline (`<p>` under brand name): `Durable, BYOK, markdown-defined agent runtime.`

#### §6.11 `HeroIllustration.tsx`
Untouched (pure SVG mark, no copy).

#### §6.12 `tests/e2e/*.spec.ts`
Audit Playwright smoke spec for visible-text assertions. Likely candidates that will fail post-rewrite:
- `Ship AI-generated PRs without babysitting the run` (Hero)
- `Connect GitHub, automate PRs` (any CTA assertion)
- `Validated PR delivery, measurable run quality...` (Home H2)
- `From queued intent to validated pull requests.` (HowItWorks H2)
- `Review a validated PR` (HowItWorks step 3)

Update each to the new strings in the same commit so CI stays green.

### §7 — Launch tweet copy (frozen May 04, 2026 — 24h pre-ship)

**Tweet draft (~360 chars; trim to 280 if not premium):**

```
UseZombie v2 ships today.

A durable, BYOK, markdown-defined agent runtime for operators who own their outcomes.

The flagship platform-ops zombie wakes on a GitHub Actions deploy failure, gathers evidence, and posts a Slack diagnosis — long after your terminal closes.

Open source. Self-host arrives in v3.

→ docs.usezombie.com/quickstart
```

**Tight version (~265 chars, fits 280 free-tier):**

```
UseZombie v2 ships today.

Durable, BYOK, markdown-defined agent runtime. The platform-ops zombie wakes on a GitHub Actions deploy failure, gathers evidence, posts a Slack diagnosis — long after your terminal closes.

Open source. Self-host in v3.

docs.usezombie.com/quickstart
```

Cross-check against architecture §0 differentiation pillars before posting:
- ✅ "open source"
- ✅ "BYOK"
- ✅ "markdown-defined"
- ✅ Wedge stated: GH Actions deploy-failure → Slack diagnosis
- ✅ Self-host posture stated: v3, not v2
- ✅ No claim of "self-hostable" / "self-host today" / "free forever"

First-screenshot freeze: a screen capture of `/usezombie-install-platform-ops` running through the 3 prompts → first Slack diagnosis arriving. Kept under `~/Projects/docs/images/launch-2026-05-XX/` for asset stability.

---

## Interfaces

```
No HTTP / DB interfaces — this milestone is docs-only.
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Docs page 404 from launch tweet link | Page not deployed yet | Verify: launch only proceeds after `curl https://docs.usezombie.com/quickstart` returns 200 |
| Architecture cross-reference finds dangling `M{N}` | Spec was renamed / merged during implementation | Update the architecture doc reference to match what shipped |
| Cold-read smoke test surfaces a confusing paragraph | Doc was written pre-ship; reality drifted | One-sentence clarification in the offending paragraph; never dilute by adding qualifiers everywhere |
| README hero drifts from architecture §0 | Independent edit on either side | `test_readme_hero_sync` catches it; fix by re-syncing both READMEs against architecture §0 |
| Marketing site Hero drifts from architecture §0 | Independent edit on `ui/packages/website/src/components/Hero.tsx` | `test_marketing_hero_pillars_match` greps for "Durable", "BYOK", "markdown-defined" tokens; CI catches the drift |
| Marketing site primary CTA points at stale `/quickstart` | PRs land marketing → architecture → docs (Captain's directive), but marketing prod-deploys before docs prod-deploys | Hold marketing prod-deploy gate until `curl https://docs.usezombie.com/quickstart` returns 200 with v2 content (grep response body for `platform-ops`). Marketing PR can merge to main first; production cutover waits |
| Forbidden v1 string survives in marketing site | Author missed a copy site during edit | `test_marketing_no_legacy_pr_framing` greps the forbidden-string list across `src/**/*.tsx`; CI fails on any hit |
| API tab references a route not in `public/openapi.json` | OpenAPI evolved post-Q3 prune | `test_api_tab_routes_resolve_in_openapi` cross-checks each `docs.json` API entry against the OpenAPI spec |

---

## Invariants

1. **No telemetry collected.** The skill never POSTs install metadata anywhere. If telemetry becomes load-bearing later, it lands behind a fresh spec with explicit privacy-doc surface — not bolted on quietly.
2. **Architecture references resolve.** Every `M{N}` reference in `docs/architecture/` has a corresponding `docs/v2/done/M{N}_*.md` file at ship time.
3. **README and architecture §0 stay synced.** Hero paragraph is byte-identical across `README.md` (this repo), `~/Projects/.github/README.md` (org profile), and the docs site landing page.
4. **Marketing site three-pillar invariant.** The string tokens `Durable`, `BYOK`, and `markdown-defined` all appear in `ui/packages/website/src/components/Hero.tsx`. Drift = CI fail.
5. **No v1 PR-validator framing in marketing source.** None of the forbidden strings (§6) appear in `ui/packages/website/src/**/*.tsx` or `tests/e2e/*.spec.ts`.
6. **API tab routes resolve.** Every group entry in `~/Projects/docs/docs.json` API tab maps to a route present in `~/Projects/usezombie/public/openapi.json`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_quickstart_page_renders` | Build docs site → assert /quickstart/index.html exists with non-empty body containing `platform-ops` and `usezombie-install-platform-ops` strings (Q-new C: collapsed to `/quickstart`, no `/platform-ops` subpath) |
| `test_concepts_context_lifecycle_renders` | Same as above for /concepts/context-lifecycle |
| `test_skills_index_renders` | Same as above for /skills |
| `test_concepts_v2_rewritten` | grep `~/Projects/docs/concepts.mdx` → no v1 framing tokens (`AI-generated PRs`, `automated PR delivery`, `validated pull request`) |
| `test_how_it_works_deleted` | `~/Projects/docs/how-it-works.mdx` does not exist; `/how-it-works` returns 404 |
| `test_no_self_host_page_in_v2` | Build docs site → assert /self-host returns 404 |
| `test_no_operator_pages_render` | Build docs site → assert every `/operator/*` URL returns 404 (16 pages hidden via `.mintignore`; files preserved on disk for v3) |
| `test_no_privacy_telemetry_page_in_v2` | Build docs site → assert /privacy/cli-telemetry returns 404 |
| `test_homelab_pages_404` | After delete, /integrations/lead-collector and /launch/homelab-zombie return 404 |
| `test_api_tab_routes_resolve_in_openapi` | Each group entry in `~/Projects/docs/docs.json` API tab parses against `public/openapi.json` (Q3 — catches `POST /v1/execute`, `GET /v1/internal/v1/telemetry`, future drift) |
| `test_arch_M_references_resolve` (folded from M50) | grep + ls — every `M{N}` mentioned in `docs/architecture/` has a corresponding `docs/v2/done/M{N}_*.md` file |
| `test_arch_anchor_links_resolve` (folded from M50) | grep `(#anchor)` style links in architecture/ → all targets exist as headers |
| `test_arch_section_14_present` (folded from M50) | After ship, `## 14. Ship Reflection` exists in architecture/ with non-empty content under 600 words |
| `test_arch_no_orphan_TODO` (folded from M50) | grep `TODO\|TKTK\|FIXME` in architecture/ → 0 hits |
| `test_readme_hero_sync` | Hero paragraph in `README.md` (this repo) is byte-identical to hero paragraph in `~/Projects/.github/README.md` |
| `test_marketing_hero_pillars_match` | grep `ui/packages/website/src/components/Hero.tsx` → contains the tokens `Durable`, `BYOK`, `markdown-defined` |
| `test_marketing_no_legacy_pr_framing` | grep `ui/packages/website/src/**/*.tsx` and `ui/packages/website/tests/e2e/**/*.ts` → 0 hits on the §6 forbidden string list |
| `test_marketing_install_command_npm` | grep `ui/packages/website/src/**/*.tsx` → 0 hits on `usezombie.sh/install.sh`; ≥1 hit on `npm install -g @usezombie/zombiectl` |
| `test_marketing_e2e_smoke` | `bun run --cwd ui/packages/website test:e2e:smoke` passes against the rewritten copy (string assertions updated in same commit) |

---

## Acceptance Criteria

### Docs site (`~/Projects/docs/`)
- [ ] `docs.usezombie.com` deploys cleanly with the v2 pages live (`/quickstart` rewritten in place, `/skills`, `/skills/usezombie-install-platform-ops`, `/concepts/context-lifecycle`, `/concepts` rewritten)
- [ ] `/quickstart` URL stays stable (Q-new C: no `/quickstart/platform-ops` subpath); v1 content overwritten
- [ ] `/how-it-works` returns 404 (page deleted)
- [ ] `/operator/*` URLs all return 404 (16 pages hidden via `.mintignore`; files preserved on disk for v3 revival)
- [ ] `/self-host` returns 404 — no v2 stub for the v3 feature
- [ ] `/privacy/cli-telemetry` returns 404 — no telemetry collected
- [ ] Docs site hero copy reflects new positioning (3 pillars: open source + BYOK + markdown-defined); homelab references gone
- [ ] API tab in `docs.json` references only routes that resolve in `public/openapi.json` (`POST /v1/execute` and `GET /v1/internal/v1/telemetry` removed; future drift caught by `test_api_tab_routes_resolve_in_openapi`)
- [ ] Three install paths documented in `/skills`: `npm install -g @usezombie/zombiectl`, host-agent skill (`/usezombie-install-platform-ops`), and `zombiectl install --from ~/.config/usezombie/samples/platform-ops`

### Marketing site (`ui/packages/website/`)
- [ ] Hero, Home, HowItWorks, FeatureFlow, FAQ, ProviderStrip, CTABlock, Agents, Pricing, Footer all carry the v2 three-pillar framing
- [ ] No forbidden v1 strings in `ui/packages/website/src/**/*.tsx` (see §6 list)
- [ ] No `usezombie.sh/install.sh` curl-pipe-bash references remain
- [ ] Hero primary CTA points at `DOCS_QUICKSTART_URL` (not `APP_BASE_URL`)
- [ ] PostHog event names unchanged (analytics continuity)
- [ ] All `data-testid` attributes preserved (e2e selector continuity)
- [ ] Animation libraries (`motion`, `BackgroundBeamsWithCollision`, `AnimatedTerminal`) untouched
- [ ] SVG zombie mark in `HeroIllustration.tsx` untouched
- [ ] Playwright e2e smoke spec passes (`bun run --cwd ui/packages/website test:e2e:smoke`) — string assertions updated to match new copy in the same commit

### Architecture (`docs/architecture/`, folded from M50)
- [ ] Cross-reference pass complete: every `M{N}` reference verified against `docs/v2/done/`
- [ ] §14 Ship Reflection added with real evidence (launch date, first external install, URLs, first real diagnosis)
- [ ] Cold-read smoke test done on `docs/architecture/`; resulting clarity fixes applied
- [ ] No orphan TODOs / FIXMEs in `docs/architecture/`

### Cross-cutting
- [ ] README hero synced: `README.md` (this repo) and `~/Projects/.github/README.md` carry the same byte-identical hero paragraph
- [ ] All tests pass (see Test Specification — count is now 19)
- [ ] **Launch-tweet copy frozen** (24h pre-ship per May 04, 2026 amendment — was 48h, compressed because the marketing rewrite folded in late): tweet copy (§7), landing-page hero (marketing site Hero.tsx + docs site index.mdx), and first-screenshot reviewed against architecture §0 differentiation pillars; sign-off captured in Ripley's Log
- [ ] Order-of-operations held: marketing site landed → architecture cross-ref + §14 landed → docs site landed → READMEs synced

---

## Out of Scope

- **Install pingback / anonymous install telemetry.** Removed from M51 scope May 03, 2026. Adoption signal comes from npm download counts, Customer Zero anecdotes, GitHub issue volume, and organic Slack DMs — building anonymous-telemetry plumbing for a dataset we don't yet need is heavier than the answer it gives. If Day-N adoption data becomes load-bearing post-launch, file a fresh spec with explicit privacy-doc surface; do not bolt it onto M51 quietly.
- **`/privacy/cli-telemetry` page.** No telemetry collected → no privacy contract needed. The page intentionally 404s in v2 (asserted by `test_no_privacy_telemetry_page_in_v2`). When telemetry lands later, this page lands with it.
- **Internal admin install dashboard.** Same reason as pingback — no data to display.
- **User-facing analytics / install metrics.** Out of scope for v2 entirely.
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

## Note on pingback removal (May 03, 2026)

The original M51 included a server-side install-pingback endpoint (`POST /v1/skills/install-pingback`), an anonymous telemetry table, an aggregation module, and an internal admin dashboard. Removed because:

1. **Heaviest piece of the milestone, lightest user value.** The operator never sees the metric. Server endpoint + schema migration + abuse controls + privacy doc + 6 tests + cross-repo coordination is genuinely substantial; Day-N adoption can be measured cheaper.
2. **Adoption signals are available without it.** npm download counts (`npm view @usezombie/zombiectl downloads`), Customer Zero anecdote, organic Slack DMs from operators who hit the launch tweet, GitHub stars / issues volume, the install-skill's smoke-test response posting to author's Slack — all give the same signal.
3. **Privacy surface inverts when collection is zero.** No collection → no `/privacy/cli-telemetry` page → less to write, less to maintain, less to audit.
4. **Reversible.** If Day-N data becomes load-bearing (e.g., we want per-OS adoption breakdown to prioritize host coverage), file a fresh spec then with the right privacy and abuse-control framing. Bolting telemetry on quietly is the failure mode this fold avoids.
