# M68_001: Trigger Registration Developer Experience (DX) + Free-Trial Pricing + "Dashboard" Rename

**Prototype:** v2.0.0
**Milestone:** M68
**Workstream:** 001
**Date:** May 13, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — wedge-launch DX blocker (cannot demo install→wire→talk in the dashboard today; pricing copy contradicts the planned launch posture).
**Categories:** API, CLI, UI, DOCS, WEBSITE, SKILL
**Batch:** B1 — single-PR landing; spec → implementation → architecture-doc updates → companion `~/Projects/docs/` Pull Request (PR).
**Branch (lead repo):** `feat/m68-trigger-dx-and-free-trial`
**Branch (companion `~/Projects/docs/`):** `feat/m68-trigger-dx-and-free-trial` (aligned name; was `chore/...` in an earlier draft — fixed)
**Depends on:**
- **M28_001** (unified webhook-auth middleware + Provider Registry — `webhook_sig`, `slack_signature`, `svix_signature`). No change required; this spec consumes it as-is.
- **M43_001** (Webhook ingest receiver — `POST /v1/webhooks/{zombie_id}/{source}`). No change required; this spec consumes it as-is.
- **M45_001** (Vault structured credentials — `zombiectl credential add --data @-`). No change required.
- **M46_001** (Frontmatter schema). Schema gets the `triggers: [...]` array shape — same Implementation file, same parser, additive evolution.
- **M49_001** (Install-skill — `/usezombie-install-platform-ops`). Step body gets rewritten for `gh`-driven registration + the chat-bubble UI parity surface.
- **M66_001** (Self-managed posture + traction rates). Free-trial rate constants land alongside the existing platform-vs-self-managed split; the strike-through is additive prose.

**Canonical architecture:**
- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.2–§8.5 — gets rewritten in this PR; the existing prose still says "paste into GitHub Settings → Webhooks" which is the DX gap this spec closes.
- [`docs/architecture/data_flow.md`](../../architecture/data_flow.md) §B Trigger — minor prose update around the four-actor envelope; no wire change.
- [`docs/architecture/scenarios/01_default_install.md`](../../architecture/scenarios/01_default_install.md) — gets a fresh walkthrough.

---

## Implementing agent — read these first

1. `docs/architecture/data_flow.md` §B — the four-actor event envelope (`steer:`, `webhook:`, `cron:`, `continuation:`). The reasoning loop never branches on actor; this stays true.
2. `samples/platform-ops/SKILL.md:217-250` — the canonical worked example of "agent reads actor field, follows prose for that actor." The pattern is load-bearing for this spec.
3. `src/zombie/config_types.zig:64-81` — current single-trigger tagged union. Promoting to `triggers: []ZombieTrigger` is the only schema seam.
4. `src/http/handlers/zombies/create.zig:69-155` — install handler. Untouched in shape; gains one response field.
5. `src/http/handlers/zombies/list.zig:68-74` — list row struct. Gains one projected sub-object.
6. `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx:46-111` — zombie detail page composition. Trigger and Live-activity sections are the two surfaces this spec rewrites.
7. `@assistant-ui/react` documentation — the runtime adapter pattern via `useExternalStoreRuntime`. The chat surface is a thin adapter over our existing Server-Sent-Events (SSE) channel.
8. `ui/packages/website/src/lib/rates.ts` + `~/Projects/docs/snippets/rates.mdx` — the two pinned files for rate display strings. Strike-through metadata lives here.

---

## Overview

**Goal (testable):**

1. **Command Line Interface (CLI):** A user running `npm install -g @usezombie/zombiectl` then `npx skills add usezombie/usezombie` inside Claude Code (or Amp / Codex / OpenCode) can type `/usezombie-install-platform-ops` and reach a working, GitHub-Actions-wired zombie posting to Slack with **zero manual paste into github.com**. The host Large Language Model (LLM) registers the webhook via the user's existing `gh` Command Line Interface authentication.

2. **Dashboard:** A user pasting any zombie's `TRIGGER.md` + `SKILL.md` into `/zombies/new` lands on `/zombies/{zombie_id}` where the Trigger panel renders a provider-specific guided card (GitHub / Linear / Jira / Grafana / Slack / agentmail / custom) — webhook URL plus pre-rendered terminal command for the user's own machine to run — and the Live-activity panel becomes a full chat surface powered by `@assistant-ui/react`: webhook / cron / continuation events render as system chips, the agent's reasoning streams as assistant bubbles, and a composer at the bottom turns user input into a steer (`actor=steer:<user>`).

3. **Pricing copy:** Until end of July 2026, every customer-visible rate string reads "Try for free" — the website's Pricing component renders the existing `$0.001` / `$0.0001` stage rates with strike-through plus a "Free until July 2026" banner; root `README.md` and `zombiectl/README.md` badges flip to "Try for free"; the `~/.github/profile/` README replaces any pricing badge with "Try for free"; `~/Projects/docs/` removes inline pricing prose and links to the website's pricing page as the single source of truth. Code constants set stage-execution charges to zero nanos for the trial window.

4. **Naming:** Every "Mission Control" string across `ui/packages/website`, `ui/packages/app`, and `~/Projects/docs` becomes "Dashboard". One vocabulary, one search hit, no aliases.

5. **Architecture documentation:** `docs/architecture/user_flow.md` §8.2–§8.5, `data_flow.md` §B trigger prose, and `scenarios/01_default_install.md` reflect the new DX so future implementers do not read stale paste-into-GitHub instructions.

**Problem:**

- The current CLI install-skill ends by telling the user to paste a webhook URL into GitHub's web settings page (M49_001 step 11). That is the longest cliff on the wedge demo and the one we eliminate.
- The dashboard install path can install a zombie and watch the activity stream live but cannot send a steer. A user who installs via the Web User Interface (UI) is stranded waiting for an external webhook fire because there is no in-page way to talk to their zombie. `src/http/handlers/zombies/messages.zig` (the steer handler) exists; only a UI client is missing.
- Single-trigger config (`ZombieConfig.trigger: ZombieTrigger`, a tagged union, in `src/zombie/config_types.zig:77-81`) prevents declaring "wake on GitHub workflow_run AND on cron" in `TRIGGER.md`. Pre-v2.0 is the only painless window to promote `trigger:` → `triggers: [...]`; per RULE NLG no compatibility shim is allowed at this prototype.
- The website's Pricing section displays full rate strings (`$0.001` / `$0.0001`) that contradict the planned launch posture ("free until July 2026 — gather traction without billing friction"). Multiple downstream surfaces — `README.md` badge, `zombiectl/README.md` badge, `~/Projects/docs/` mdx pages — all repeat the `$5` starter-credit string. Centralising on "Try for free" eliminates the drift surface and lets future bumps happen in exactly two files (`rates.ts` + `rates.mdx`).
- "Mission Control" was once an aspirational rename for the dashboard surface; "Dashboard" is the term actually used in `docs/`, in zombiectl prose, and in user conversation. The mixed vocabulary surfaces in search results and onboarding copy. Pick one — Dashboard.

**Solution summary:**

The spec is **one PR** with five mostly-independent slices that share `/zombies/{id}` as a fate-shared surface.

A. **API additions (zero new endpoints).** `POST /v1/workspaces/{ws}/zombies` 201 response gains `webhook_urls: { <source>: <url> }`. List endpoint projects `triggers: [{source, events}, ...]` from `config_json`. Schema gains an `events: ?[][]const u8` field on each trigger and an enclosing array.

B. **CLI install-skill rewrite.** Steps S1.0 (precondition check) and S1.8–S1.10 (parse `triggers`, call `gh api repos/.../hooks`, Hash-based Message Authentication Code (HMAC) self-verify) replace today's "print and paste" tail. The skill is platform-neutral — it runs in any host that has Claude Code's `AskUserQuestion` equivalent.

C. **Dashboard chat surface.** `@assistant-ui/react` replaces the bespoke `LiveEventsPanel`. A `useZombieEventStream` hook adapts the existing SSE channel to assistant-ui's runtime model. Webhook / cron / continuation events render as `system`-role chips via custom message renderers. The composer's `onNew` callback wires to `POST /v1/.../zombies/{id}/messages` (existing handler).

D. **Trigger panel goes multi-card.** `TriggerPanel.tsx` renders one card per declared trigger in `zombie.triggers[]`. Card variants: `GuidedTriggerCard` (known webhook provider; pre-renders terminal registration command), `CopyUrlCard` (unknown source; today's behaviour as fallback), `CronCard` (schedule + next fire), `ApiCard` (catch-all `POST /v1/zombies/{id}/events` ingress).

E. **Pricing strike-through + Dashboard rename + architecture docs refresh.** Website Pricing component renders rates with strike-through plus "Free until July 2026" banner. Pictorial onboarding-flow component lands directly under Pricing on the home page. Constants flip to zero nanos for stage execution. Every "Mission Control" string becomes "Dashboard". `docs/architecture/` topic files get the new DX prose. Companion PR to `~/Projects/docs/` ships the docs-site equivalents.

The receiver substrate (M43_001 + M28_001) is untouched. No new entity, no new endpoint, no new database column, no migration. Schema changes are JSON-projection only (existing `config_json` jsonb column carries the new fields).

---

## Files Changed (blast radius)

### A. API + config (5 files)

| # | File | Action | Why |
|---|---|---|---|
| A1 | `src/zombie/config_types.zig:77-81` | EDIT | `ZombieConfig.trigger: ZombieTrigger` → `ZombieConfig.triggers: []ZombieTrigger`. Tagged union per element stays. Add `events: ?[]const []const u8 = null` to the `webhook` variant. |
| A2 | `src/zombie/config_helpers.zig` | EDIT | Parse `x-usezombie.triggers` as required array, length 1–8. Validate uniqueness on `(type, source)` tuple; ≤1 `cron`; ≤1 `api`. Parse `events:` per webhook entry; cap 16 elements, ≤64 chars each. Reject singular `trigger:` (RULE NLG: fail loud, no compat shim — error `ERR_ZOMBIE_INVALID_CONFIG` with message `use "triggers:" (array)`). |
| A3 | `src/zombie/config_test.zig` | EDIT | Add tests for the array shape, the uniqueness rules, the events cap, and the loud rejection of legacy singular shape. |
| A4 | `src/http/handlers/zombies/create.zig:150` | EDIT | Replace `webhook_url` (currently absent in response but expected by CLI) with `webhook_urls: { <source>: <url> }` map. Derived as `${API_ORIGIN}/v1/webhooks/${zombie_id}/${source}` per webhook trigger; empty map when no webhook triggers declared. |
| A5 | `src/http/handlers/zombies/list.zig:68-74` | EDIT | `ZombieListRow` gains `triggers: ?[]TriggerSummary` projected via `SELECT config_json->'triggers'` and post-projected to `[{source, events, type}, ...]`. |
| A6 | `zombiectl/src/commands/zombie.js:118` | EDIT | Replace `webhook_url: res.webhook_url` with `webhook_urls: res.webhook_urls` in the JSON-mode output. Non-JSON mode gains a trailing block listing each registered URL. The CLI today reads a field that doesn't exist server-side; this edit closes that latent bug alongside the wire change. |
| A7 | `src/http/handlers/zombies/list.zig` + handlers/zombies/events.zig | EDIT | Add `actor_prefix` optional query parameter to `GET /v1/.../zombies/{id}/events`. Server-side filter only; no client-side fallback. The UI in §6 depends on this being live (no two-track conditional). |
| A8 | `src/http/handlers/zombies/api_integration_test.zig` | EDIT | Pin test for `actor_prefix=webhook:` filter. |

### B. Install-skill (3 files)

| # | File | Action | Why |
|---|---|---|---|
| B1 | `skills/usezombie-install-platform-ops/SKILL.md` | EDIT | The current skill body has steps 1–12. After this spec: (i) prepend a numbered "0. Preconditions" subsection collecting today's prereqs (npm install, npx skills add, `zombiectl auth login`, `gh auth login -s admin:repo_hook`) plus the new precondition check (`which zombiectl && which gh && zombiectl doctor --json`) — replaces today's "Installation" section header; (ii) keep step numbering 1–12 unchanged for steps that are unchanged; (iii) rewrite step 7 (install) to capture `webhook_urls`; (iv) insert a new step 8 (parse rendered `TRIGGER.md` `triggers[]`); (v) rewrite step 9 (loop `gh api repos/${GH_REPO}/hooks` per webhook entry, with substituted URL + secret + events); (vi) rewrite step 10 (HMAC self-verify each registered URL); (vii) rewrite step 11 (post-install summary — drop paste-into-GitHub prose, list each registered hook); (viii) keep step 12 (smoke-test steer) verbatim. Net: same 12 steps + the new "0. Preconditions" subsection. ~+50 / −40 lines. |
| B2 | `skills/usezombie-install-platform-ops/references/credential-resolution.md` | EDIT | Add a `gh auth status` precondition + recovery hint `gh auth refresh -s admin:repo_hook`. |
| B3 | `skills/usezombie-install-platform-ops/references/failure-modes.md` | EDIT | Add three rows: `gh` missing scope (403/401 → exact refresh command), `gh api 422 Hook already exists` (idempotent — `gh api repos/.../hooks` list, match on `config.url`, advance), `gh api 404 Not Found` (repo or token wrong; stop). |

### C. Sample template (1 file)

| # | File | Action | Why |
|---|---|---|---|
| C1 | `samples/platform-ops/TRIGGER.md` | EDIT | Convert `trigger:` (singular) → `triggers:` (one-element array). Add `events: ["workflow_run"]` to the webhook entry. |

### D. Dashboard chat surface (8 files)

| # | File | Action | Why |
|---|---|---|---|
| D1 | `ui/packages/app/package.json` | EDIT | Add `@assistant-ui/react` as a caret range (`^x.y.z` against the latest stable on npm at implementation time — Captain decision 2026-05-15: caret, not exact pin, to let patch fixes float). Pull Request body records the resolved version after `bun install`. Imports go through subpath entries (`@assistant-ui/react/runtime`, `@assistant-ui/react/primitives`) to preserve tree-shaking. The route-level mount in D7 is a dynamic import (`next/dynamic`, `ssr: false`) so assistant-ui's runtime + primitives ship in a separate chunk and the `/zombies/[id]` first-load JS only pays for it once the chat surface mounts client-side. |
| D2 | `ui/packages/app/components/domain/ZombieThread.tsx` | NEW | Mounts `AssistantRuntimeProvider`, `<Thread />`, `<Composer />`. Consumes `useZombieEventStream` and `steerZombie`. |
| D3 | `ui/packages/app/components/domain/useZombieEventStream.ts` | NEW | Hook: initial backfill via `listZombieEvents(limit:50)`; opens `EventSource` against `GET /v1/.../zombies/{id}/events/stream`; folds SSE frames into an assistant-ui `Message[]` array; maps actors → roles (`steer:` → `user`; agent chunks → `assistant`; `webhook:` / `cron:` / `continuation:` → `system` with `customData`). |
| D4 | `ui/packages/app/components/domain/zombieMessageRenderers.tsx` | NEW | Custom `MessagePrimitive` renderers for `system`-role messages. Per-actor styling: webhook chip ("↪ webhook:github · workflow_run failure · kishore/usezombie@c0a151bd", collapsible to full `request_json`); cron chip ("↻ cron:0 */30 * * * · 18:30Z"); continuation chip ("↩ continuation · <reason>", muted). |
| D5 | `ui/packages/app/components/domain/SteerComposer.tsx` | NEW (thin wrapper) | Wraps assistant-ui's `<Composer />` with our design-system `<Textarea>` + `<Button>` slots, disables while a stage is `received` (running), surfaces optimistic-state styling. ~40 lines. |
| D6 | `ui/packages/app/lib/api/zombies.ts` | EDIT | Add `steerZombie(workspaceId, zombieId, message, token) → Promise<{event_id}>` posting `{message}` to `POST /v1/workspaces/{ws}/zombies/{id}/messages`. |
| D7 | `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx:96-100` | EDIT | Replace `<LiveEventsPanel ...>` with a `next/dynamic` import of `ZombieThread` (`ssr: false`, with a thin skeleton fallback matching the panel's grid cell). The dynamic boundary keeps assistant-ui out of the route's initial JS chunk; D7's verify step asserts the static-import variant is NOT used. |
| D8 | `ui/packages/app/components/domain/LiveEventsPanel.tsx` | DELETE | Replaced by `ZombieThread`. Per RULE NLR, remove not deprecate. |
| D8b | `ui/packages/app/tests/events-components.test.ts` | EDIT | Remove `LiveEventsPanel` import + tests; cover the equivalent paths via `ZombieThread.test.ts` (D3 family). Verified empirically: `grep -rln LiveEventsPanel ui/packages` returns three files — D7 (page.tsx swap), D8 (the file itself), D8b (this test). No other importers. |

### E. Trigger panel multi-card (5 files)

| # | File | Action | Why |
|---|---|---|---|
| E1 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | EDIT | Switch from single-tab Webhook/Schedule to a list of cards: `zombie.triggers.map(t => <Card variant={t.type, t.source} t={t} />)`. Add footer prose pointing at "edit TRIGGER.md and re-install" as the source-of-truth model. |
| E2 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.ts` | NEW | Static `PROVIDER_GUIDANCE: Record<Source, GuidanceCard>` map. Entries for `github`, `linear`, `jira`, `grafana`, `slack`, `agentmail`. Each defines: title, events-label formatter, terminal-command template, web-User-Interface deep-link template, user-input variable list (e.g. `OWNER/REPO`, `TEAM_ID`, `WORKSPACE`). |
| E3 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/GuidedTriggerCard.tsx` | NEW | Renders State B (known provider). Composes events label, webhook URL with Copy button, rendered command block with Copy button, web-UI deep link, last-delivery line. Pure presentational. |
| E4 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/CronCard.tsx` | NEW | Renders cron triggers. Shows the schedule, next-fire computed client-side (timezone-aware), and links the user to Recent Activity filtered `actor LIKE 'cron:%'`. ~50 lines. |
| E5 | *(was ApiCard.tsx — removed)* | — | `type: api` is carved out for v1 (see Out of Scope). Zombies whose declared source isn't in `PROVIDER_GUIDANCE` fall through to `CopyUrlCard`. ApiCard ships with the workspace-API-tokens spec, not this one. |

### F. UI types + tests (4 files)

| # | File | Action | Why |
|---|---|---|---|
| F1 | `ui/packages/app/lib/types.ts:16-22` | EDIT | `Zombie` gains `triggers?: TriggerSummary[]` where `TriggerSummary = {type: "webhook"\|"cron"\|"api", source?: string, events?: string[], schedule?: string}`. |
| F2 | `ui/packages/app/components/domain/zombieMessageRenderers.test.tsx` | NEW | Snapshot test per actor: webhook chip, cron chip, continuation chip, gate-blocked chip. |
| F3 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.test.ts` | NEW | Snapshot test per provider: given `triggers[0] = {source, events}` + webhook URL, the rendered command and deep-link strings match a fixture. Pins prose. |
| F4 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.test.ts` | NEW | (a) renders one card per trigger; (b) unknown source → falls back to `CopyUrlCard`; (c) last-delivery line populates from `listZombieEvents(actor_prefix, limit:1)`. |

### G. Pricing strike-through + free-trial (10 files)

| # | File | Action | Why |
|---|---|---|---|
| G1 | `ui/packages/website/src/lib/rates.ts` | EDIT | Add `FREE_TRIAL_UNTIL = "2026-07-31"`, `FREE_TRIAL_END_MS = 1785542400000`, `FREE_TRIAL_STAGE_NANOS = 0n`. Add `RATES_DISPLAY.STAGE_PLATFORM_STRUCK` and `RATES_DISPLAY.STAGE_SELF_MANAGED_STRUCK` mirroring today's strings. Add `RATES_DISPLAY.HEADLINE = "Try for free"`. |
| G2 | *(moved to section L — see L1)* | — | The docs-repo `snippets/rates.mdx` change lives in the companion Pull Request, not this lead-repo PR. Cross-repo write from this worktree is forbidden by CLAUDE.md. |
| G3 | `ui/packages/website/src/components/Pricing.tsx` | EDIT | Render existing rate strings with `<s>` (strike-through, design-system token). Add a callout block above: "Free until {FREE_TRIAL_UNTIL} — every event and stage execution is on us while we gather traction. Self-managed posture still recommended for production-grade isolation." Strike applies to both platform-default and self-managed rate lines. CTA badge changes from "→ try free · $5 starter credit, never expires" to "→ try free · free until July 2026". Audit the full 174-line file for any other `STARTER_CREDIT` use during the edit; spec verified only line 34 today, but the audit happens at implementation. |
| G3b | `ui/packages/website/src/components/FAQ.tsx:23` | EDIT | Rewrites the pricing Q&A to reference the canonical Pricing section: drop the per-stage rate prose; replace with "Free to try until July 31, 2026 — see the Pricing section for current rates and trial details. Stealth-mode testing rate; rates rise post-General-Availability (GA)." |
| G3c | `ui/packages/website/src/pages/Terms.tsx:42` | EDIT | Replace the rate-itemized ListItem with: "usezombie is free to try through July 31, 2026 (UTC); see the Pricing section on the home page for current rates and trial period. Each new account receives a starter credit; specific value displayed on the Pricing page." Terms can keep dynamic-rate framing without locking a number. |
| G4 | `ui/packages/website/src/components/Pricing.test.tsx` | EDIT | Add test for strike-through render + the free-trial banner copy. Add tests for FAQ + Terms text changes (`Pricing.test.tsx` may need siblings `FAQ.test.tsx` + `Terms.test.tsx` audited too — both already exist). |
| G5 | `ui/packages/website/src/components/OnboardingFlow.tsx` | NEW | Pictorial 4-step DX guidance, rendered as a Section immediately under Pricing on `Home.tsx`. Steps: (1) `npm install -g @usezombie/zombiectl` + `npx skills add usezombie/usezombie`; (2) Run `/usezombie-install-platform-ops` in Claude (or paste `TRIGGER.md` + `SKILL.md` in the Dashboard); (3) Wire the webhook (`gh api` one-liner pre-rendered; or copy the command from the Dashboard); (4) Steer the zombie ("howdy" from terminal `zombiectl steer` or from the Dashboard chat composer). Each step is a horizontally-laid card with an icon + label + a code snippet + a sub-caption. ~180 LOC, no images — typography + design-system tokens only. |
| G6 | `ui/packages/website/src/components/OnboardingFlow.test.tsx` | NEW | Snapshot test for the four cards; deterministic rendering. |
| G7 | `ui/packages/website/src/pages/Home.tsx` | EDIT | Mount `<OnboardingFlow />` immediately under `<Pricing />`. One line. Verify `<Pricing />` is mounted on `Home.tsx` at implementation time; spec assumed but didn't confirm. |
| G8 | `README.md` (root) | EDIT — line 7 + 20 | Replace `[![Try Free — $5 Credit](...)](...)` badge with `[![Try for free](...)](...)`; in line 20 prose, drop "`$5` starter credit on signup, no card required" — replace with "Free to try, no card required". |
| G9 | `zombiectl/README.md` | EDIT — line 5 + body | Same badge swap. Delete any inline rate references; replace with "See [usezombie.com/#pricing](https://usezombie.com/#pricing) for live pricing." |
| G10 | `~/Projects/.github/profile/README.md` | NO EDIT NEEDED IN THIS REPO | Confirmed clean of `$5` references; only "Install" + "Docs" badges. Out of scope for this PR. Recorded here so a future audit doesn't reopen. |
| G11 | `ui/packages/website/src/components/Hero.tsx:64-70` (primary CTA) | EDIT | The "→ install in Claude Code" button currently `<a href={DOCS_QUICKSTART_URL}>` — sends the user off-page to read prose. Replace with a `<button>` whose `onClick` (a) writes `npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie` to `navigator.clipboard`, (b) shows a 2-second "Copied — paste into your terminal" toast (use existing design-system `<Toast>` if present; otherwise a plain `aria-live` region), (c) smooth-scrolls to the `#onboarding-flow` anchor on the same page (the new section from G5). One click, one paste, no nav. Keep `DOCS_QUICKSTART_URL` as a small tertiary "read the full quickstart →" link inside OnboardingFlow itself (G5). Update `Hero.test.tsx:52-56` accordingly. |
| G12 | `ui/packages/website/src/components/Hero.tsx:74-85` (secondary CTA "view a real wake (replay)") | EDIT | Currently routes to `/agents`, which is an API-surface page aimed at AI crawlers — no replay UI lives there. Re-target to `/replay` (new route added in G13). |
| G13 | `ui/packages/website/src/pages/Replay.tsx` | NEW | A new top-level route that plays back a real platform-ops session: install transcript → webhook arrives → tool calls → response posts → Slack message. Time-coded frames replayed by a small client-side state machine. No backend dependency. Frames are checked in at `ui/packages/website/src/data/replay-platform-ops.json` (G14). One small "Restart" button + a progress scrubber. Total runtime ~75 seconds, then loops. Page is a single Section composing existing design-system `<Terminal>` + `<LogLine>` + an inline chat-bubble specimen (mirrors what the Dashboard's ZombieThread will look like — pulls double duty as a preview). |
| G14 | `ui/packages/website/src/data/replay-platform-ops.json` | NEW | Canned frame data: `[{ts_ms, kind: "log" \| "tool_call" \| "chunk" \| "system", text, severity?}, ...]`. Captured at implementation time from a real platform-ops install + webhook fire + agent response. Anonymised: repo names replaced with `your-org/your-repo`, secrets redacted. Frame count ≤200. |
| G15 | `ui/packages/website/src/pages/Replay.test.tsx` | NEW | (a) renders all frames in order; (b) progress scrubber jumps to a frame; (c) "Restart" rewinds to ts_ms=0; (d) loops after the final frame; (e) page doesn't crash if `replay-platform-ops.json` is empty. |
| G16 | `ui/packages/website/src/App.tsx` (or wherever react-router routes live) | EDIT | Add `<Route path="/replay" element={<Replay />} />`. |
| G17 | `ui/packages/website/src/components/Hero.test.tsx` | EDIT | Update test assertions: primary CTA is a button (not `<a>`), clicking it triggers clipboard.writeText with the bootstrap one-liner, and an anchor scroll fires. Secondary CTA's `to` is `/replay`, not `/agents`. |

### H. Code constants (3 files)

| # | File | Action | Why |
|---|---|---|---|
| H1 | `src/state/tenant_billing.zig` | EDIT | Add `FREE_TRIAL_STAGE_NANOS = 0` constant. Wire `compute_stage_charge` to return `FREE_TRIAL_STAGE_NANOS` while `is_free_trial_active(now_ms)` returns true. `is_free_trial_active` returns `now_ms < FREE_TRIAL_END_MS` where `FREE_TRIAL_END_MS = 1785542400000` (2026-08-01 00:00:00 UTC — i.e. end of July 31 inclusive). After that timestamp, fall through to the existing `STAGE_PLATFORM_NANOS` / `STAGE_SELF_MANAGED_NANOS` behaviour. No schema change. |
| H2 | `src/state/tenant_billing_test.zig` | EDIT | Pin tests: stage charge during trial = 0; after trial = existing rates. Time is injected, not read from clock. |
| H3 | `zombiectl/src/constants/billing.js` | EDIT | Add `FREE_TRIAL_STAGE_NANOS = 0`, `FREE_TRIAL_END_MS = 1785542400000`. Identifier parity rule across Zig/TS/JS keeps three files in lockstep. |
| H4 | `ui/packages/app/lib/types.ts` | EDIT | Same constants mirrored. |
| H5 | `ui/packages/website/src/lib/rates.ts` | EDIT | Mirror `FREE_TRIAL_STAGE_NANOS` and `FREE_TRIAL_END_MS`. Already in G1; cross-listed here for the parity-pin test. |
| H6 | **NEW — Free-trial state surface** | NEW | (a) `zombiectl doctor --json` `billing` block gains `free_trial: { active: bool, ends_at_ms: int }`. (b) Dashboard billing panel (`ui/packages/app/app/(dashboard)/page.tsx` or sibling billing surface) renders a single line "Free trial · expires 2026-07-31 (UTC)" when active. Without this surface, users start getting charged Aug 1 with no prior signal. |
| H7 | `src/http/handlers/billing/tenant_billing.zig` (existing) | EDIT | Project the free-trial block into the response shape the dashboard + doctor both consume. |

### I. Documentation removal — internal pricing prose (4 files)

| # | File | Action | Why |
|---|---|---|---|
| I1 | `~/Projects/docs/concepts.mdx` | EDIT (companion PR) | Delete the inline rates paragraph; replace with a one-line link to `https://usezombie.com/#pricing`. |
| I2 | `~/Projects/docs/workspaces/overview.mdx` | EDIT (companion PR) | Same. |
| I3 | `~/Projects/docs/billing/plans.mdx` | EDIT (companion PR) | Delete rate tables; redirect to website. Keep the concept paragraph (what a stage is, what an event is). |
| I4 | `~/Projects/docs/billing/budgets.mdx` | EDIT (companion PR) | Same. |

### J. "Mission Control" → "Dashboard" rename (≥4 files; full sweep at implementation time)

| # | File | Action | Why |
|---|---|---|---|
| J1 | `ui/packages/app/app/layout.tsx` | EDIT | "Mission Control" in the page title / shell becomes "Dashboard". |
| J2 | `ui/packages/app/tests/app-components.test.ts` | EDIT | Test assertions on "Mission Control" become "Dashboard". |
| J3 | `ui/packages/website/src/components/FeatureFlow.tsx` | EDIT | One occurrence in the prose. |
| J4 | `~/Projects/docs/` files (companion PR) — at least: `quickstart.mdx`, `zombies/webhooks.mdx`, `api-reference/error-codes.mdx`, `AGENTS.md`, `changelog.mdx`, `zombies/overview.mdx`, `workspaces/managing.mdx`, `cli/zombiectl.mdx` | EDIT | Each occurrence rewritten as "Dashboard". |
| J5 | Implementation note | INVARIANT | Implementer runs `grep -rln "Mission Control" .` (full repo, plus the docs-repo at companion-PR time) at start of work AND before PR open; both runs return empty. Lead-repo grep covers `src/`, `zombiectl/`, `ui/`, `samples/`, `skills/`, `docs/`, `README.md`, `CHANGELOG.md`. Docs-repo grep covers `~/Projects/docs/`. Empirical baseline today: 4 hits — `ui/packages/app/app/layout.tsx`, `ui/packages/app/tests/app-components.test.ts`, `ui/packages/website/src/components/FeatureFlow.tsx`, plus docs-repo files. Final list captured in PR Discovery section per PR. |

### K. Architecture documentation refresh (4 files)

| # | File | Action | Why |
|---|---|---|---|
| K1 | `docs/architecture/user_flow.md` §8.2–§8.5 | EDIT | Rewrite §8.2 (Install) to mention the npm bootstrap + skill bootstrap; §8.3 (Triggering) to mention the gh-driven webhook registration; §8.4 (Iteration) to mention the Dashboard chat surface; §8.5 (Platform-Ops example) to follow the new 12-step flow. |
| K2 | `docs/architecture/data_flow.md` §B Trigger | EDIT | Add `events: [...]` field shape to the webhook envelope description. No diagram changes. |
| K3 | `docs/architecture/scenarios/01_default_install.md` | EDIT | Full rewrite. Walks the new four-step pictorial flow (bootstrap → install-skill → wire → steer) as one continuous transcript. |
| K4 | `docs/architecture/README.md` glossary table | EDIT | Add a row for "Trigger panel" + a row for "Free-trial pricing". Update the "Webhook trigger" row to reference the array shape. |
| K5 | `docs/architecture/billing_and_provider_keys.md` | EDIT | Add a "Free-trial window" subsection in the credit-pool description: while `now_ms < FREE_TRIAL_END_MS`, `compute_stage_charge` returns 0; receive charge stays 0 as today; starter credit still granted on signup as a balance line item for continuity post-trial. No changes to the credential / posture / model-cap prose. |

### N. Internal Clerk endpoint rename — auth plane vs customer-data plane separation (4 files + 1 ops step)

The current `POST /v1/webhooks/clerk` (`src/http/router.zig:95-97`) is **internal** — Clerk emits `user.created` to it so we provision a new tenant row. It has nothing to do with customer zombies. Sitting it in the same `/v1/webhooks/` namespace as the customer-data-plane receiver (M43) blurs auth-plane vs data-plane separation and pollutes `PROVIDER_REGISTRY` with a special-case branch. Pre-v2.0 (RULE NLG) is the only painless window to rename.

| # | File | Action | Why |
|---|---|---|---|
| N1 | `src/http/router.zig:36, 95-97` | EDIT | Rename the route variant from `clerk_webhook` to `auth_identity_event_clerk`. Move the catch-all path-match from `"/v1/webhooks/clerk"` to `"/v1/auth/identity-events/clerk"`. The `/v1/webhooks/` namespace becomes customer-data-plane only. |
| N2 | `src/http/handlers/webhooks/clerk.zig` (current location) → `src/http/handlers/auth/identity_events_clerk.zig` | MOVE | The handler moves with the route. Imports updated. Internal-only — no customer impact. |
| N3 | `src/http/handlers/auth/identity_events_clerk_test.zig` | EDIT | Path assertion + test name updated (RULE TST-NAM clean — no Milestone identifier). |
| N4 | `src/http/route_table.zig` + `src/http/route_table_invoke.zig` | EDIT | Wire the renamed route variant. |
| N5 | Clerk dashboard webhook URL | OPS STEP (recorded in PR description) | Update our Clerk app's webhook endpoint from `https://api.usezombie.com/v1/webhooks/clerk` to `https://api.usezombie.com/v1/auth/identity-events/clerk`. Deployment-time action, not code. PR description carries the exact value to paste; operator confirms before merging the API change. Order: deploy API → update Clerk dashboard → verify next `user.created` lands → merge. |
| N6 | `public/openapi/paths/auth/identity-events.yaml` | NEW | Per REST_API_DESIGN_GUIDELINES §6 (tag 1:1 with resource), placed in a new `paths/auth/` subdirectory. Introduces the subdirectory pattern because the auth namespace is about to grow (identity-events today; sessions, tokens, identities, oauth callbacks when zombie-auth lands). Existing flat files (`paths/admin.yaml`, `paths/authentication.yaml`, `paths/webhooks.yaml`, etc.) are left alone in this PR — moving `paths/authentication.yaml` → `paths/auth/authentication.yaml` is a sibling cleanup recorded in Discovery, not bundled here. Tag: `auth-identity-events`. Documents `POST /v1/auth/identity-events/clerk`: request body shape (Clerk's payload, pass-through), Svix headers, 202 / 401 / 422 responses, idempotency on `svix-id`. File ≤400 lines per §6. |
| N7 | `public/openapi/root.yaml` | EDIT | (a) Add the `auth-identity-events` tag declaration and the `$ref` to `paths/auth/identity-events.yaml`. (b) **Remove four stale tag declarations**: `Runs` ("Core spec-to-PR pipeline" — leftover from an earlier product framing; zero hits in `src/` or `docs/architecture/`), `Execute`, `Slack`, `Telemetry`. All four are orphans — confirmed via tag-vs-paths-file audit at spec time: none has a `paths/*.yaml` file, none is referenced from any path operation in other yaml files. They render as dead-end navigation entries on docs.usezombie.com via the `x-mintlify.navigation` annotations. RULE NLR touch-it-fix-it since this file is already being edited. The cleanup is mechanical: delete each tag block (4 lines per tag including the `x-mintlify` block) — recorded in Discovery with exact line ranges removed. |

**Forward compatibility note for zombie-auth.** The namespace was chosen so the next auth-plane spec can land cleanly without renaming anything:

```
/v1/auth/identity-events/clerk         ← this PR
/v1/auth/identity-events/zombie-auth   ← future, same static-segment pattern
/v1/auth/sessions, .../tokens, .../identities, .../oauth/...
                                       ← zombie-auth own resources, same /v1/auth/ root
```

The workspace-API-tokens spec (deferred from M68; admits `type: api` triggers) has its pre-allocated home at `/v1/auth/tokens`. No further URL churn expected on the auth plane post-launch.

**Customer-facing Clerk path is unchanged.** `/v1/webhooks/svix/{zombie_id}` (M28) stays exactly as-is and remains the PROVIDER_GUIDANCE['clerk'] target. That path is customer-data-plane and belongs in `/v1/webhooks/`; the internal one does not.

**Verification:** `grep -rln "/v1/webhooks/clerk" .` returns empty before PR open. Receiver tests for the new path pass. The internal `user.created` flow re-bootstraps cleanly on a manual signup test.

### O. Redis client audit (read-only research dimension — 1 deliverable)

Standalone read-only audit of `src/queue/*.zig` against two third-party Zig Redis libraries. Deliverable is a single markdown report at `src/queue/AUDIT.md`. **No code edits inside this dimension** — purely a research artifact whose recommendations will seed a follow-up implementation spec. Lives in this M68 PR for proximity to the wider scope-bundle the user is shipping together; could have been a separate spec but the user chose to dimension it here.

**Motivation.** `src/queue/redis_client.zig` carries a single `std.Thread.Mutex` on `Client` that all command paths (XADD, XACK, XAUTOCLAIM, PUBLISH) contend on. Worker write-path + progress callbacks all hit one lock; M42_003's contention diagnosis is the existing record. Two reference libraries with mature pooling + concurrency models exist in the OSS ecosystem and can be read for patterns we should adopt.

| # | Path | Action | Why |
|---|---|---|---|
| O1 | `src/queue/AUDIT.md` | NEW | Single markdown report. Sections: Executive summary (3–5 bullets: what to steal, what to fix, what to keep); Per-dimension analysis with file:line citations from reference libs; Concrete recommendations for usezombie ranked P0/P1/P2; Specific code patterns to adopt (pseudocode acceptable, exact when obvious). |

**Read-only target code (DO NOT edit in this dimension):**
- `src/queue/redis_client.zig` — the `Client` struct (~255 lines)
- `src/queue/redis_transport.zig` — plain + TLS transport
- `src/queue/redis_pubsub.zig` — dedicated subscriber conn
- `src/queue/redis_zombie.zig` — zombie stream ops (XREADGROUP etc.)
- `src/queue/redis_config.zig` — URL parsing, CA bundle
- `src/queue/redis_protocol.zig` — RESP serializer/deserializer
- `src/queue/redis_types.zig` — `RedisRole` enum
- `src/queue/redis.zig` — facade re-export

**Read-only reference code (DO NOT edit; cite by path + line range):**
- `~/Projects/oss/redis.zig/` — karlseguin's redis.zig
- `~/Projects/oss/zig-okredis/` — zig-okredis

**Eight dimensions to audit:**

1. **Allocation patterns.** Per-command allocations (arena? pooled bufs?), response value lifetime (ownership + free point), buffer sizing strategy (read/write buffers, static vs dynamic).
2. **Connection pooling.** Pool init / acquire / release / deinit. Connection lifecycle (health check on acquire? eager vs lazy connect?). Eager `connect_on_init` count (redis.zig does this). Pool-to-result lifetime coupling (`result.deinit` releases conn?).
3. **Concurrency model.** **The key issue.** usezombie has one `std.Thread.Mutex` on `Client` — every command contends. How do redis.zig and zig-okredis structure locking (per-connection mutex? lock-free? sharded pools?). What's the right model for the usezombie usage pattern (one pool → many zombies → each does XADD / PUBLISH / XACK)?
4. **Fault tolerance + retry.** Reconnect strategy. Write-phase vs read-phase retry. Idempotency awareness (XADD / PUBLISH replay safety). Connection health detection (SO_KEEPALIVE, heartbeat, PING). Error surfacing (errdefer, error payloads, PG-style err object). Stale connection detection. Pool repair (invalid connections returned to pool → close + reopen).
5. **Stability + reliability.** Invariants enforced (e.g. drain before deinit). Edge cases (half-open connections, Redis restart, Upstash proxy timeout). TLS handshake failure recovery. Memory leak guards (errdefer on partial init). Pub/sub disconnection behaviour.
6. **Performance.** Per-command allocator pressure. Buffer reuse across commands. Unsafe fast paths (redis.zig has `row.getUnsafe` / `nextUnsafe`). Prepared statement / pipelining support. Where the M42_003 contention bottleneck bites.
7. **Pooling return patterns.** redis.zig's `pool.query → result.deinit` auto-release. How zig-okredis handles the same. What usezombie does today vs should do.
8. **What usezombie does that neither library handles** (these stay — focus the audit on client infrastructure, not on these business-logic bits): per-zombie stream consumer groups (`zombie_workers`); `XREADGROUP` / `XAUTOCLAIM` / `XACK` lifecycle; pub/sub subscriber with `SO_RCVTIMEO` heartbeat; role-based ACL env vars (`REDIS_URL` vs `REDIS_URL_API` vs `REDIS_URL_WORKER`).

**Hard constraints on the audit:**

- No code edits anywhere in the repo or in the reference libs.
- No recommendation to switch libraries — the custom streams / pub-sub code stays.
- No speculation without a reference-lib citation (`file.zig:LN-LN`).
- Recommendations ranked P0/P1/P2 explicitly so the follow-up implementation spec can pick a slice.

This dimension can land independently of the rest of M68's slices because it produces only a markdown artifact. Execution order: schedule after §1–§9 (architecture, config, install/list, install-skill, internal Clerk rename, pricing, READMEs, Mission Control sweep, chat surface, trigger panel) but before `make` verification — the audit doesn't affect the verification surface.

### M. Release mechanics (3 files — lead repo)

| # | File | Action | Why |
|---|---|---|---|
| M1 | `VERSION` | EDIT | Bump per release-template.md voice. M68 is a minor-feature workstream landing alongside trigger DX + free-trial; pick the bump level at implementation time based on whether the wire change to install-response counts as breaking (it does for unreleased v2 callers; treat as part of the v2.0 ramp). |
| M2 | `build.zig.zon`, `zombiectl/package.json`, `zombiectl/src/cli.js` | EDIT | Propagated by `make sync-version` per CLAUDE.md. Implementer runs `make sync-version` after the VERSION edit; pre-commit `make check-version` passes. |
| M3 | `CHANGELOG.md` (or `~/Projects/docs/changelog.mdx` via companion PR per repo convention) | EDIT | New `<Update>` entry following `~/Projects/dotfiles/skills/release-template.md` voice — Mintlify-style headline, lead paragraph stating the change, bold-lead-noun bullets, no marketing words. Two surfaces here: a brief lead-repo CHANGELOG.md note if the repo carries one, and the canonical docs-site `changelog.mdx` entry in the companion PR. |

### L. Companion PR to `~/Projects/docs/` (separate Pull Request)

This PR opens a paired PR in the docs repository. Files in section I + J4 + G2 are listed here for traceability but land in `~/Projects/docs/` — not in this lead-repo PR. Per CLAUDE.md "Docs-repo edits on own branch" — branch `feat/m68-trigger-dx-and-free-trial`, off `main` in the docs repo (branch name aligned with the lead-repo branch for cross-PR readability).

| # | File (in docs repo) | Action |
|---|---|---|
| L1 | `snippets/rates.mdx` | Add free-trial banner + strike-through display strings. |
| L2 | `concepts.mdx`, `workspaces/overview.mdx`, `billing/plans.mdx`, `billing/budgets.mdx` | Strip inline rate prose; link to website. |
| L3 | All "Mission Control" → "Dashboard" rewrites. |
| L4 | New page `zombies/install.mdx` rewritten with the four-step pictorial flow (mirrors `OnboardingFlow.tsx` shape). |
| L5 | `quickstart.mdx` updated to lead with `npm install -g @usezombie/zombiectl`. |
| L6 | `zombies/webhooks.mdx` rewritten: gh-driven registration, the per-provider command map, the Dashboard chat composer. |
| L7 | `changelog.mdx` — new `<Update>` entry per the project's changelog voice (canonical reference in `~/Projects/dotfiles/skills/release-template.md`). |

---

## Applicable Rules

- **RULE NLG** — No legacy framing pre-v2.0.0. The singular `trigger:` shape is rejected loudly; no rewrite-on-load shim. Free-trial code path is `is_free_trial_active(now)` with a real timestamp, not a feature flag named `legacy_` anything.
- **RULE NDC** — No dead code at write time. `LiveEventsPanel.tsx` is deleted in D8 once `ZombieThread` lands.
- **RULE NLR** — Touch-it-fix-it. Files edited here that carry stale prose (e.g. "Mission Control" comments, "$5 starter credit" docstrings) get cleaned up in the same diff.
- **RULE ORP** — Cross-layer orphan sweep. The renamed `Zombie.trigger` → `Zombie.triggers`, the deleted `LiveEventsPanel`, and the `Mission Control` strings must return zero grep hits across Zig / TS / JS / docs / fixtures before the PR opens.
- **RULE NTE** — No type erasure. The `TriggerSummary` projection in the list response is a typed Zig struct with a typed TypeScript mirror.
- **RULE NSQ** — Named constants. `FREE_TRIAL_END_MS`, `FREE_TRIAL_STAGE_NANOS`, `MAX_TRIGGERS_PER_ZOMBIE = 8`, `MAX_EVENTS_PER_TRIGGER = 16`, `MAX_EVENT_NAME_LEN = 64`. No magic numbers in validation.
- **RULE UFS** — String reuse. The webhook URL template lives in one constant; the per-provider command templates live in `provider-guidance.ts` and are tested against fixtures.
- **RULE CTM / RULE CTC** — Constant-time HMAC compare. M28's middleware already enforces this; no change here.
- **RULE FLL** — Files ≤ 350 lines. `provider-guidance.ts` is data-heavy; if it crosses 350, split per provider into `provider-guidance/{github,linear,jira,grafana,slack,agentmail}.ts` with a `mod.ts` aggregator.
- **RULE TST-NAM** — Test filenames + `test "…"` names carry no Milestone identifiers. None of the new tests embed `M68` / `§4.2` / `T7`.
- **SCHEMA GUARD** — No schema change. `config_json` is jsonb; the `triggers` array is JSON-level only. No migration row in `src/cmd/common.zig`.
- **DESIGN TOKEN GATE** — Every new `.tsx` file in `ui/packages/app` and `ui/packages/website` uses tokens from `@usezombie/design-system`. `@assistant-ui/react` primitives are wrapped (D5) so token discipline holds at the wrapper boundary.
- **DOC READ GATE** — `docs/REST_API_DESIGN_GUIDELINES.md` for A4/A5 response-shape additions; `docs/DESIGN_SYSTEM.md` for D2–D5 + E1–E5 + G5; `docs/BUN_RULES.md` for the assistant-ui dependency; `docs/AUTH.md` for D6 (steer endpoint authentication).
- **SPEC TEMPLATE GATE** — This file. No time/effort estimates, no complexity ratings, no percentage-complete, no owners.

---

## Sections (implementation slices)

Ordered for landing within one PR. Earlier sections do not block later ones at the file level, but the PR commit history follows this order so reviewers can read it sequentially.

### §1 — Config: promote to array + `events`

A1 + A2 + A3 + C1. Schema-level seam. Lands first because every subsequent slice consumes the new shape.

Validation rules:
- `triggers` is required; length 1–8.
- For each entry, `type` is `webhook` | `cron` | `api`.
- For `webhook`: `source` is required, non-empty, ≤32 chars, matches `^[a-z][a-z0-9_-]*$`. `events` optional; if present, 1–16 elements, each non-empty, ≤64 chars, no whitespace.
- For `cron`: `schedule` is required, must parse as a 5-field cron expression. At most one `cron` entry per zombie.
- `type: api` is **not accepted in v1**. The tagged union in `config_types.zig` retains the `api: void` variant (no schema change to the in-memory shape), but the parser rejects it with `ERR_ZOMBIE_INVALID_CONFIG` and message `type: api is not yet available — use webhook or cron`. Carve-out reason in Out of Scope. Re-admission lands with the workspace-API-tokens spec.
- Across entries: unique on `(type, source)` tuple. Two `webhook:github` entries → `ERR_ZOMBIE_INVALID_CONFIG` with message `duplicate trigger (type, source) tuple`.
- Singular `trigger:` (legacy) → `ERR_ZOMBIE_INVALID_CONFIG` with message `use "triggers:" (array)` and Discovery hint pointing at this spec.

Edit-in-place migration: every fixture under `samples/`, `samples/fixtures/`, every `_integration_test.zig` with inlined `TRIGGER.md`, the install-skill template at `~/.config/usezombie/samples/platform-ops/TRIGGER.md` (postinstall copy of `samples/platform-ops/TRIGGER.md`). RULE NLG window — no compat shim. Final orphan sweep proves zero singular `trigger:` references remain.

### §2 — Install + List response shape

A4 + A5 + F1. `webhook_urls` map on 201; `triggers` projection on list. Type mirrors. No new endpoint.

### §3 — Install-skill rewrite

B1 + B2 + B3. Twelve steps; S1.0 is new (precondition), S1.7 captures `webhook_urls` from JSON, S1.8 parses rendered `TRIGGER.md` for `triggers` + `events`, S1.9 loops `gh api` per webhook trigger, S1.10 HMAC self-verifies each, S1.11 final summary lists each registered hook. No paste-into-GitHub prose remains.

Failure-mode reference rows for `gh api` 403/404/422 — exact recovery hints inline.

### §4 — Architecture documentation refresh

K1 + K2 + K3 + K4. Lands in this slice (not at end) so reviewers reviewing §1–§3 can cross-reference up-to-date prose. Per CLAUDE.md "Architecture Consult & Update Gate": doc landing rule is "doc-only commit OR same-commit; **never** after code."

### §5 — Dashboard chat surface

D1 + D2 + D3 + D4 + D5 + D6 + D7 + D8. The `@assistant-ui/react` dep arrives in D1; the rest builds on it. D8 deletes `LiveEventsPanel.tsx` only after D7 swaps the import in `page.tsx`.

Bundle-size discipline: `@assistant-ui/react` is imported with explicit subpath imports (`@assistant-ui/react/runtime`, `@assistant-ui/react/primitives`) to keep tree-shaking honest. No default markdown component — `react-markdown` skipped in favour of our existing design-system Markdown renderer. Bundle budget for `/zombies/[id]` route: implementer measures the current route's gzipped bundle as the first action of §5, records it in Discovery, then asserts the post-change bundle does not exceed `current + 100 kB gz`. The 100 kB head-room is the working ceiling for assistant-ui's runtime + primitives + adapter; if breached, escalate before merge.

API-version discipline: `useExternalStoreRuntime` is the documented adapter at the time of writing, but assistant-ui is pre-1.0 with version churn. Implementer verifies the exact hook name + signature + message shape against the pinned version's published API at implementation time. If the hook has been renamed (e.g. `useLocalRuntime`, `useThreadRuntime`) or the `Message` type has changed shape, adapt §5's prose and note the deviation in Discovery.

Custom message renderers (D4) are pure presentation, no fetches. Webhook chip is collapsible — collapsed shows summary line, expanded shows full `request_json` formatted as JSON code block (uses existing design-system `<Code>` component).

### §6 — Trigger panel multi-card

E1 + E2 + E3 + E4 + E5 + F2 + F3 + F4. Static guidance map; one snapshot test per provider locks the rendered command.

`provider-guidance.ts` schema (TypeScript). Seven providers ship with this spec — all of M28's registry. Clerk shares the same card shape as Slack: no widely-installed CLI exists for its webhook creation, so the card renders a web-UI deep link to the Clerk dashboard plus a numbered checklist. The card variant is generic across deep-link providers; only the data table differs.

```typescript
type Source = "github" | "linear" | "jira" | "grafana" | "slack" | "agentmail" | "clerk";
type GuidanceCard = {
  title: string;                                   // e.g. "GitHub Actions"
  formatEventsLabel: (events: string[] | null) => string;
  variables: Variable[];                           // user inputs above the command
  commandTemplate: (ctx: TemplateCtx) => string;   // rendered with WEBHOOK_URL, events, vault refs
  webUiDeepLink: (vars: Record<string, string>) => string;
  credentialFields: {                              // referenced via zombiectl credential get
    secretField: string;                           // e.g. ".webhook_secret"
    tokenField?: string;                           // e.g. ".api_token"
  };
};
```

`Variable` is `{name, label, placeholder, optional?}`. Each card renders the variable inputs above the command block; the command re-renders client-side as the user types. Nothing leaves the page — no API call for command rendering.

Last-delivery line uses existing `listZombieEvents` with a new server-side query parameter `actor_prefix` (A7 in section A). No client-side fallback — the server filter ships in this same PR, so the UI depends on it directly. Half-finished implementations are prohibited per CLAUDE.md.

### §7 — Pricing strike-through + free-trial constants + onboarding flow

G1 + G3 + G4 + G5 + G6 + G7 + H1 + H2 + H3 + H4.

Pricing component strikes both `STAGE_PLATFORM` and `STAGE_SELF_MANAGED` strings; banner reads exactly: "Free until July 31, 2026 — every event receipt and stage execution is on us while we gather traction. Self-managed posture still recommended for production-grade isolation." CTA badge: "→ try free · free until July 2026".

`OnboardingFlow.tsx` renders four horizontally-laid cards on desktop, stacked on mobile. Each card carries:
- Numbered chip (1 / 2 / 3 / 4)
- Title ("Install the CLI" / "Run the install skill" / "Wire your trigger" / "Steer your zombie")
- Code snippet block (monospace, with copy button, design-system `<Code>` component)
- One-line sub-caption

The four exact snippets:

1. `npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie`
2. `/usezombie-install-platform-ops` (in Claude Code; alternative shown: paste `TRIGGER.md` + `SKILL.md` in the Dashboard at `/zombies/new`)
3. `gh api -X POST repos/<OWNER/REPO>/hooks --field 'events[]=workflow_run' --field "config[url]=<WEBHOOK_URL>" --field "config[secret]=<SECRET>"` (full form shown collapsed)
4. `zombiectl steer <zombie_id> "morning health check"` (alternative shown: type into the Dashboard composer)

Code constants flip in H1–H4; identifier parity rule keeps Zig/TS/JS in lockstep. Pin test in `tenant_billing_test.zig` asserts free-trial path returns zero, post-trial path returns existing rates.

### §8 — Mission Control → Dashboard rename

J1 + J2 + J3 + J4 + J5. Implementer's first action: `grep -rln "Mission Control" ui/ ~/Projects/docs/`. Rewrite every match. Implementer's last action before PR: same grep — must return empty. Final list captured in PR Discovery.

### §9 — README + zombiectl/README + companion docs PR

G8 + G9 + L1–L7.

Lead-repo PR ships G8 + G9. Companion PR ships L1–L7 — opened from `~/Projects/docs/` on branch `chore/m68-trigger-dx-free-trial`. Same operator opens both PRs; merge order is companion-then-lead so the lead-PR's `README.md` link points at the live updated docs page. Cross-reference: lead-PR description carries the companion-PR link; companion-PR description carries the lead-PR link.

### §10 — Synthetic system events (architecture extension) — DONE

Shipped at `612dfb79` (docs-only): `docs/architecture/data_flow.md §Synthetic system events` + `user_flow.md §8.3` crossref + `bastion.md` "What does not change" crossref. Establishes the single-publisher invariant for one-shot state changes (config reload, future `balance_exhausted` / `manual_pause` / `kill_received`): inline private write fn next to the state-changing fn, durable row in `core.zombie_events`, no activity-channel publish. Codex-validated (Option D, durable-only).

### §10a — Durable system-event row on config reload — DONE

Shipped at `331819d6`. `reloadZombieConfig` extended (`SELECT` now returns `updated_at`) + private `writeReloadEventRow` (~28L). Idempotent via `cfg-{revision}` event_id + `ON CONFLICT DO NOTHING`. Four integration tests in `event_loop_reload_emits_system_row_integration_test.zig` via the existing pub `reloadZombieConfig` test seam. No activity-channel publish; durable-only.

### §10b — PATCH body fields + row-lock field-merge

Extends `PATCH /v1/workspaces/{ws}/zombies/{id}` to accept `{trigger_markdown?, source_markdown?}` alongside the existing `{status?, config_json?}`. No new endpoint; no `:verb` operation (REST §1 collision check forbids it — `status` already holds the values). No optimistic-concurrency token — see "what we do NOT add" below.

**Transaction shape (row-lock + field-level merge).** All field-bearing PATCHes run inside one transaction:

```
BEGIN;
SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '10s';
SET LOCAL idle_in_transaction_session_timeout = '5s';
SELECT config_json, updated_at, status
  FROM core.zombies
  WHERE id = $1 AND workspace_id = $2
  FOR UPDATE;                                                       -- one row, one lock
-- in Zig: reparse trigger_markdown / source_markdown if present;
-- overlay halves onto current config_json via patch_merge.zig helper
UPDATE core.zombies
  SET config_json = $merged, status = COALESCE($status, status), updated_at = $now
  WHERE id = $1 AND workspace_id = $2
    AND status != 'killed' AND (<fsm-guards-as-today>)
  RETURNING updated_at;
COMMIT;
```

The handler then XADDs `config_changed` to `zombie:control` so the worker reloads → §10a system-event row fires for free.

**Concurrency contract.** Two writers patching *different* body fields (`{trigger_markdown}` + `{source_markdown}`) both land via the row-lock merge — no silent clobber. Two writers patching the *same* field collapse to last-write-wins (the second reads the first's commit, overlays, writes).

**Deadlock invariant.** The §10b txn locks **exactly one row** (the `core.zombies` row identified by `$id`) and performs **exactly one UPDATE** on that same row. No second `SELECT FOR UPDATE`, no second table touched, no second row referenced. Deadlock requires a lock-ordering cycle; a one-row txn cannot participate in one. Future edits to `patch.zig` that add any second locked read/write must re-run deadlock analysis and update the integration test matrix.

**What we do NOT add: ETag / `If-Match`.** Optimistic-concurrency tokens are an HTTP-client-state mechanism — they only buy anything for a long-lived client that carries server-state across multiple requests (browser fetch → user edits for 2 min → save sends the original token). M68 has no such client:

- The Dashboard work in §5 is a chat surface (steer events), not a config editor — it doesn't write `trigger_markdown` / `source_markdown` at all.
- The CLI is launched fresh per invocation — no prior server-state to be stale against; LWW is the right semantic.
- The install-skill is CI/CD-driven; the repo is the source-of-truth.

Adding `ETag` headers, `If-Match` honoring, 412 surfaces, and a stale-revision error code for clients that don't exist is build-ahead-of-need. **When a Dashboard config-editor ships in a future spec, ETag/If-Match becomes a focused add-on in that spec — server-side honor + browser auto-send — validated against the real client.** Until then, row-lock + field-merge is the entire concurrency story for `core.zombies`.

**Defensive timeouts (per-txn, not session-wide).** `lock_timeout=5s` makes a long lock-wait fail fast with Postgres `55P03` rather than tie up a pool connection; `statement_timeout=10s` bounds the txn; `idle_in_transaction_session_timeout=5s` kills the connection if the client disconnects mid-merge. Mapped to `503 ERR_DB_LOCK_TIMEOUT` (retryable) at the handler.

CLI: `zombiectl zombie update <id> --from <dir>` reads TRIGGER.md + SKILL.md from `<dir>`, PATCHes the existing route. (`update` is the operator-facing verb; `patch` is HTTP plumbing — this matches the `install` / `list` imperative shape.) No concurrency-token flag — fresh-launch invariant makes one meaningless; LWW with row-lock-merge is the contract.

### §10c — install-skill update-in-place

Markdown-only branch in `skills/usezombie-install-platform-ops/SKILL.md`: when a zombie already exists for the repo+workspace, route step 7 through `zombiectl zombie update` (the §10b CLI) instead of `zombiectl install`. ~30 lines markdown + eval test.

### §11 — Dashboard chat surface polish bundle

Extends §5's D1–D8b chat surface with accessibility, loading affordances, motion, and responsive behavior. Pure UI dimensions; no API surface or schema impact.

- **D9 — ARIA `role="log"` on viewport.** `ThreadPrimitive.Viewport` carries `role="log"`, `aria-live="polite"`, `aria-label="Live activity"`. Screen readers announce frame arrivals without interrupting the user.
- **D10 — Backfill skeleton.** When `connectionStatus ∈ {CONNECTING, RECONNECTING}` and `events.length === 0`, render three `Skeleton` rows in place of the "Waiting for activity" hint. Avoids the dead-pixel feel during the initial backfill window.
- **D11 — Frame-enter fade-in.** Every rendered row carries `animate-in fade-in-0 duration-150`. Streaming rows keep the same DOM node across CHUNK updates so the animation only fires on first paint, not on every concat.
- **D12 — Jump-to-latest button.** `ThreadPrimitive.ScrollToBottom` mounts inside the Card; auto-hides at bottom via `disabled:invisible`. Mono `↓ latest` label, project token sizing.
- **D13 — Responsive md/sm modifiers.** Actor rail collapses under the body at `<md` via a `--actor-rail-w` CSS variable + `GRID_2`/`GRID_3` constants (single source of truth for the literal `72px`, derived for the webhook payload offset via `calc(var(--actor-rail-w)+var(--actor-rail-gap))`). Composer button stacks below the textarea at `<sm`. Webhook `<pre>` caps at `max-h-64`.
- **D14 — Polish tests.** Nine vitest specs in `tests/zombie-thread.test.ts` pin: ARIA attribute presence; skeleton render gate (CONNECTING/RECONNECTING with 0 events, suppressed when LIVE or events present); frame-enter class on every `data-role` row; scroll-to-bottom mount; responsive grid + `--actor-rail-w` injection; composer flex-col-to-sm-row breakpoint.

Token cleanup landed alongside this section: session-introduced parallel naming (`text-body-sm`/`font-sans`/`leading-body`/`text-text-subtle`) reverted to dashboard convention (`text-sm`/`text-muted-foreground`); project-native mono/semantic tokens (`text-mono`, `text-label`, `text-evidence`, `font-mono`, `leading-mono`) preserved per the locked v2-B visual.

### §12 — Persistent zombie thread subscription (registry)

Lifts the streaming subscription out of `useZombieEventStream`'s per-mount `useEffect` lifecycle into a module-level registry keyed by `zombieId`. Solves the "dashboard ↔ /zombies/[id] round-trip reconnects every visit" DX bug: within a 30s idle window after the last consumer detaches, the `EventSource` survives and the next mount re-subscribes against the live entry — zero reconnect, zero re-backfill.

App Router note: a `/zombies/[id]/layout.tsx` host would only survive *within* the segment (sub-routes that don't exist yet), not across exits to `/dashboard`. The module-singleton registry is the level above the layout tree that actually persists for the DX case Captain flagged.

- **D15 — Frame transform split.** Pure helpers (`mergeBackfill`, `applyLiveFrame` family, `actorToRole`) extracted to `lib/streaming/zombie-stream-frames.ts` (130 LOC). Keeps the registry file under the 350L LENGTH GATE without losing the per-file coherence.
- **D16 — Registry primitive.** `lib/streaming/zombie-stream-registry.ts` (292 LOC). Exports `subscribe(workspaceId, zombieId, token, listener) → unsubscribe`, `getSnapshot(zombieId)`, `appendOptimistic`, `reconcileOptimistic`, `__resetRegistryForTests`. Refcounted per-zombie entries; SSE opens cookie-authed regardless of token; backfill skipped when token is null and resumed if a later subscriber arrives with one; reconnect backoff (1s→15s, cap 5 attempts) preserved from the original hook; 30s idle release after refcount hits zero.
- **D17 — React hook as registry consumer.** `useZombieEventStream` rewritten as a thin `useSyncExternalStore` boundary (94 LOC). Public API preserved exactly — `events`, `connectionStatus`, `isRunning`, `retryState`, `appendOptimistic`, `reconcileOptimistic`, `convertEvent` — so `ZombieThread.tsx` and 442 existing tests are untouched.
- **D18 — Registry behavior tests.** `lib/streaming/zombie-stream-registry.test.ts` (12 specs) pins: single EventSource per zombieId regardless of subscriber count; listener fanout on snapshot change; refcount keeps connection alive when one subscriber detaches; idle timer (not immediate close) on refcount-zero; idle release tears down after 30s+1ms; **same-zombie revisit within idle window does NOT open a new EventSource** (the load-bearing assertion); cross-zombie nav opens a fresh stream; null-token opens SSE + skips backfill; later token kicks off the deferred backfill; optimistic append/reconcile + no-op on never-subscribed zombieId.
- **D19 — E2E surface pin.** `tests/e2e/acceptance/zombie-thread.spec.ts` — two Playwright specs: live-activity Card + composer renders for an authenticated user (smoke); dashboard ↔ /zombies/[id] round-trip preserves the surface (DX). Deeper SSE-injection tests (first-frame timing, CHUNK growth, reconnect badge recovery, steer roundtrip) deferred — need backend test-mode frame-emit hooks not in M68 scope; filed as follow-ups in the spec docstring.

### §13 — zombiectl login redesign (Tiers 1–3)

Redesigns `zombiectl/src/commands/core.js commandLogin` per the CLI login DX critique. Pure-CLI improvements landing here in M68; auth-flow-shape changes (ECDH transport, out-of-band verification code, server-side device labeling, token introspection endpoint, server-side revocation) split to a sibling spec — see `HANDOFF_SUPABASE_HARDENING_SPEC.md` at the worktree root. That sibling owns the **handshake redesign**; §13 here owns only the **CLI behavior polish** that doesn't depend on the new handshake.

zombiectl already does polling (not paste-the-code), so Tier-1 items A (loopback callback) and #7 (verification-code retry semantics) from the Supabase critique table don't apply. The dimensions below cover what *does* translate.

In-scope this PR (CLI-only, no handshake changes):

- **D22 — TTL countdown (C).** Replace the silent spinner with `Session expires in MM:SS` updating each poll tick. Deadline derived client-side from `Date.now() + timeoutSec * 1000` since the response shape today carries no `expires_at_ms` (the cli-auth handshake hardening spec will add a server-supplied deadline; until then client-derived is the source).
- **D23 — Fail-loud workspace hydration (D).** Surface `hydrateWorkspacesAfterLogin` failures with a stderr warning, not the current silent `catch { return null; }`. Exit code stays 0 — login succeeded, workspace hydration is best-effort. The existing unit-test contract (line 145–172 + 174–204 of `test/login.unit.test.js`) explicitly pins exit 0 when hydration fails, and breaking that would mis-signal login failure to scripts.
- **D27 — Decompose `commandLogin` (H). — DONE.** Refactored the 142-line monolith into six named stages — `resolvePollParams`, `createLoginSession`, `announceLoginSession`, `maybeOpenBrowser`, `pollUntilComplete`, `persistAndHydrate`, `emitLoginResult` — plus a 45-line orchestrator. `validate` deliberately omitted (D24 is deferred to the cli-auth handshake hardening follow-up; RULE NDC bans empty-scaffold stages). External signature `commandLogin(ctx, parsed, workspaces, deps)` preserved verbatim; 625/625 unit tests green; every surface the acceptance suite asserts (`login_url:` regex, exit codes 0/1/130, `login complete` line, credentials.json shape + 0600 mode, no-token-in-stdout) unchanged. File 225 LOC (cap 350); every fn ≤ 50.
- **D28 — Error taxonomy (I).** Distinguish `InvalidSession` / `ExpiredSession` / `NetworkError` / `RateLimited` / `Timeout` / `Interrupted`. Tightens the existing AUTH_PRESET map. No new error pathways — purely a remap of conditions zombiectl already encounters.
- **D29 — Exp-backoff polling with jitter (J).** Start 1s, grow to 5s, ±20% jitter, honor server `Retry-After` if sent. Caps polling RPS during retry storms.
- **D30 — Polling transient-retry (K).** Single 503 / network blip during the poll loop doesn't kill the session — log + continue.
- **D31 — `zombiectl auth status` companion (L). — DONE (un-deferred per Captain).** Landed client-only: no new server endpoint required. Resolves token source (file vs env vs none), decodes JWT claims via the existing `decodeTokenPayload` helper (`iss`/`aud`/`sub`/`metadata.tenant_id`/`metadata.role`/`exp`), then probes `GET /v1/tenants/me/billing` (existing authenticated endpoint per AUTH.md test infra) as the validity check. Probe outcomes — `valid` (200), `unauthorized` (UZ-AUTH-001/002 or TOKEN_EXPIRED/UNAUTHORIZED tag), `unreachable` (network / 5xx) — drive exit code: 0 for valid|unreachable (token still trusted; transient API issues don't poison local state), 1 for unauthorized. Defensive `probe ?? { status: "unreachable" }` guard added per Captain's null-safety review since zombiectl is plain JS with no type checker. Eight unit tests in `test/auth-status.unit.test.js` pin: no-creds-no-env → exit 1; file-with-valid → exit 0 + tenant_id/role/server_check rendered; env-only → source=env + saved_at null; unauthorized → exit 1; unreachable → exit 0; expired JWT → token.expired=true; file token wins over env (file source); malformed JWT → token=null but probe still runs. New `auth` subcommand group declared in `cli-tree.js` (under top-level Commands, before `doctor`); `auth status` listed in the Subcommands help block. Golden help file regenerated.

Deferred to **cli-auth handshake hardening** sibling spec (`HANDOFF_SUPABASE_HARDENING_SPEC.md`):

- **D20 — Idempotency check (A).** Overlaps with the sibling spec's "already-logged-in detection" hardening; lands cohesively with the new handshake UX rather than as a one-off here.
- **D21 — Token name flag (B).** Overlaps with the sibling spec's "token name / device label" hardening; the JWT-vs-row labeling tension (Clerk JWTs are stateless, nothing to label server-side without schema work) is a design decision the sibling spec owns.
- **D24 — Token validation before save (E).** The `/me` ping endpoint shape is decided by the handshake redesign (today no auth introspection endpoint exists — Flow 3's `core.api_keys` is service-to-service, separate from Flow 1's Clerk JWT path).
- **D25 — Argv-leak warning (F).** `--token` flag doesn't exist in `cli-tree.js` today; adding it is itself a new auth pathway ("direct token mode") that the sibling spec's multi-mode token resolution will introduce.
- **D26 — TTY-priority env resolution (G).** Same reason as D25 — `ZMB_TOKEN`/`ZOMBIE_TOKEN` env-token reading doesn't exist in commandLogin today; it lands with the new pathways.
- **D32 — `zombiectl logout --all` (M).** Needs the server-side revocation design the sibling spec resolves (Clerk JWTs are stateless; revocation is non-trivial).

### §14 — TypeScript strict migration (zombiectl + cross-package alignment)

**Why now:** zombiectl is plain JS with no type checker — the D31 review surfaced `probe.status` could-be-null risk that no static check would catch. Captain elevated this to in-PR scope (nearing production; null/undefined bugs must be statically caught). Strict TypeScript across the whole CLI is the production-readiness gate. Cross-package `package.json` versions also drifted (`design-system` had caret `^6.0.3` vs exact `6.0.3` elsewhere; `website` was missing `@types/node`); aligning them is part of this workstream so the type-engine surface is consistent.

Mechanic: tsc as the static checker (matches the UI packages' engine). Source uses `.ts` extensions in relative imports; `allowImportingTsExtensions: true` + `rewriteRelativeImportExtensions: true` in `tsconfig.json` means the eventual emit will rewrite to `.js` for Node ESM distribution. Worktree dev/test loop spawns the CLI via `bun` (handles mixed `.js`/`.ts` imports transparently during the migration); global/published mode keeps using plain `node` against the emitted artifact.

Strict settings: `strict: true`, `noUncheckedIndexedAccess: true`, `exactOptionalPropertyTypes: true`, `noImplicitReturns: true`, `noUnusedLocals: true`, `noUnusedParameters: true`, `useUnknownInCatchVariables: true`. Catches the bug classes Captain called out (`probe.status` on possibly-null `probe`, `array[i]` on possibly-undefined, missing `?.` chains).

- **D33 — Foundation + cross-package alignment. — DONE.** Added `typescript@6.0.3`, `@types/node@^25.8.0`, `@types/uuid@^11.0.0`, `bun-types@^1.3.13` as devDeps to `zombiectl/package.json`. Added `typecheck` script + wired `tsc --noEmit` into `lint`/`lint:fix`/`build`. Created `zombiectl/tsconfig.json` with the full strict set above. Aligned `design-system/package.json` (pinned `typescript`/`@types/react`/`@types/react-dom` to exact versions matching app/website; added `typecheck` script; added `@types/node`). Added `@types/node` to `website/package.json`. Both `bun.lock`s regenerated.
- **D34 — `src/constants/**/*.{js,ts}` migration. — DONE.** Renamed all ten constants files (`analytics-events`, `auth-roles`, `billing`, `cli-errors`, `cli-flags`, `doctor-checks`, `error-codes`, `event-status`, `signals`, `zombie-status`) from `.js` to `.ts` via `git mv` (history preserved). Added types where strict mode required: `formatDollars(nanos: number | null | undefined): string`. Updated 17 consumer files across `src/`, `test/`, `bin/` to import via `.ts` extension. Worktree-mode CLI spawner switched from `node` to `bun` in `test/acceptance/fixtures/cli.js` (cross-extension import resolution). `scripts/audit-runtime-imports.mjs` extended to walk `.ts` files and accept `.ts` as a valid Node-ESM-equivalent extension during the migration window. 633/633 tests + lint + typecheck all green.
- **D35 — `src/util/**` + `src/output/**` migration. — DONE.** All 6 files renamed `.js → .ts` via `git mv` (history preserved): `util/url`, `output/{capability,palette,glyph,format,index}`. Type discipline established for the shared rendering layer:
  - `output/capability.ts` exports `ColorModeValue` union, `IsTtyStream` + `WritableStreamLike` interfaces (the latter is the minimum-viable contract any stream-writer consumer needs).
  - `output/palette.ts` exports `Token` union (`"pulse" | "evidence" | ...`), `StyleOpts` interface, `Painter` function type, `Palette` interface. Internal `XTERM_256` + `BASIC_16` lookup tables now `Record<Token, number>` / `Record<Token, string>` so `noUncheckedIndexedAccess` doesn't fire on the indexed access (token is type-narrowed by the caller).
  - `output/glyph.ts` exports `GlyphInstance`, `GlyphFactory`, `GlyphSet` so commands can type-annotate the spinner/status surface.
  - `output/format.ts` exports `FormatOpts`, `TableColumn`, `TableRow`, `KeyValueRows`. The `widthHint` typing made the prior implicit-any `{}` opts object explicit.
  - `output/index.ts` exports `UiTheme` interface (covers the `ui.ok/err/warn/...` proxy), `WriteStream` interface for `print*` helpers, and re-exports all the upstream types so command modules can `import type { StyleOpts, FormatOpts, TableColumn } from "../output/index.ts"`.
  - `util/url.ts` adds explicit `string | null | undefined` signature on `normalizeApiUrl`.
  - 13 consumer imports updated across `src/` (cli.js, ui-progress.js, program/{help,banner,handlers-bind}.js) and `test/` (output-{capability,format,glyph,palette}.unit.test.js, output-and-io-fill.unit.test.js, help.unit.test.js, coverage-fill.unit.test.js).
  - `test/output-invariants.unit.test.js` updated: `walk()` scans both `.js` and `.ts`; `PALETTE_PATH` points to `palette.ts`; expected callsites in the pulse-currency invariant updated to `glyph.ts` + `format.ts`. The invariant still asserts pulse-cyan is consumed from at most 3 source files — proved that the migration didn't accidentally scatter pulse usage.
  - 633/633 tests still green. File LOC unchanged from pre-migration source.
- **D36 — `src/program/auth-token` + `src/lib/state` migration.** DONE.
  - `src/program/auth-token.ts` exports `JwtClaims` (with nested `JwtMetadata`, `JwtRoleClaim`), `RoleClaim = "user" | "operator" | "admin"`. All three extractors take `unknown` so callers can pass `string | null | undefined` (or anything else) without widening at the call site; tests already exercised non-string inputs. `decodeTokenPayload` returns `JwtClaims | null`; the JSON-parse boundary uses `as JwtClaims` (the only cast in the file).
  - `src/lib/state.ts` exports `Credentials`, `WorkspaceItem`, `Workspaces`, `StatePaths` interfaces. `readJson<T>` is now a generic with `as T` at the JSON boundary; the ENOENT/SyntaxError catch narrows via `err as { code?: unknown; name?: unknown }` (TS strict's `useUnknownInCatchVariables` requires the structural check). `null` is the canonical absent-value across persisted fields — no `undefined` in the on-disk schema.
  - 11 consumer imports updated: `src/cli.js`, `src/commands/auth.js`, and 9 test files (`auth-token.unit.test.js`, `state.unit.test.js`, `coverage-fill.unit.test.js`, `cli-alignment.unit.test.js`, `workspace-add.test.js`, `helpers-cli-state.js`, `api-url-resolution.integration.test.js`, `onboarding-flow.integration.test.js`, `failure-modes.integration.test.js`). All point at `.ts` paths; the JS-imports-TS pattern is unchanged from prior waves.
  - 633/633 tests still green. `auth-token.ts` 100% function coverage; `state.ts` 100% function coverage (96.55% line, identical to pre-migration `.js`).
- **D37 — `src/lib/http` + `src/lib/sse` + `src/program/http-client` migration.** DONE.
  - `src/lib/http.ts` (327 lines) exports `ApiError` (class), `ApiErrorDetails`, `RetryReason` union, `RetryConfig`, `AttemptInfo`, `RetryInfo`, `FetchImpl`, `ApiRequestOptions`, `ApiRequestWithRetryOptions`, `AuthCredentials`. `apiRequest` + `apiRequestWithRetry` + `authHeaders` typed end-to-end. `classifyRetryable` accepts `unknown` and narrows via `instanceof ApiError`, `instanceof TypeError`, and structural object-shape checks; `hasRetryOptOut` replaces the original `err.body?.error?.retry_after_seconds === 0` (which TS rejected on `body: unknown`) with an explicit narrowed walk. Error-response shape lifted to `ErrorEnvelope` + `isErrorEnvelope` type-guard.
  - `src/lib/stream-fetch.ts` (112 lines, NEW). POST-based SSE consumer extracted from `http.js` to keep `http.ts` under the 350-line FLL cap. Owns `streamFetch` + `SseEvent` + `StreamFetchOptions` + an internal `parseSseFrame`. Mirror module to `lib/sse.ts` (which owns the GET transport). `ApiError` + `FetchImpl` imported from `http.ts` so the two transports share their error vocabulary. **Captain decided this split** when the FLL gate fired — no re-export shim in `http.ts`; callers import from `stream-fetch.ts` directly.
  - `src/lib/sse.ts` (137 lines) exports `streamGet` + `parseSseFrame` + `SseFrame` + `StreamGetOptions` + `StreamGetCallback`. Replaced the original `json?.error?.code || json?.error_code` lookup with a typed `readNestedString` helper that walks an `unknown` value safely. `onEvent` typed as `(event) => boolean | void` (returning `false` halts the stream).
  - `src/program/http-client.ts` (69 lines) exports `HttpRequestContext` + `apiHeaders` + `request`. `HttpRequestContext` is the minimal context surface the HTTP layer reads (`apiUrl`, optional `token`/`apiKey`/`fetchImpl`/`retryConfig`/`analyticsClient`/`distinctId`); command modules pass a richer object via structural typing. Full ctx widening is D40's concern.
  - `src/lib/analytics.d.ts` (NEW). Ambient declaration for the still-JS `analytics.js` module so the TS→JS import seam at `http-client.ts → analytics.js` typechecks honestly. Declares `trackHttpRequest` + `trackHttpRetry` + the other exports `cli.js` reads. Migrates to real `.ts` when `analytics.js` itself migrates.
  - **`exactOptionalPropertyTypes` learnings, baked in:** `AuthCredentials.{token,apiKey}` typed as `string | null | undefined` (not `string | null`) so callers can pass `ctx.token` (whose type already includes `undefined`) without a guard at every call site; `ApiRequestWithRetryOptions.retry` widened to accept `null | undefined` since callers use `ctx.retryConfig ?? null` to signal "use defaults"; `fetchImpl?: FetchImpl | undefined` for the same reason. The principle: parameter types accept the actual shape callers produce; tighten at the boundary if narrower types make sense (e.g. `apiRequestWithRetry` internally turns `null` into `{}`).
  - 12 consumer imports updated: `src/cli.js`, `src/lib/run-command.js`, `src/commands/zombie_steer.js`, and 9 test files (`helpers.js`, `run-command.unit.test.js`, `error-matrix.unit.test.js`, `request-retry.unit.test.js`, `http-retry.unit.test.js`, `streamfetch.unit.test.js`, `sse.unit.test.js`, `sse-streamget.unit.test.js`). `src/lib/run-command.js` was missed by the first grep sweep (relative path `./http.js` didn't match the broader pattern); oxlint's `import(extensions)` rule surfaced it at lint time, [[feedback_blast_radius_grep]] note absorbed.
  - 633/633 tests still green. `http.ts` 94.12 → 92.92 line coverage (same as pre-migration `.js`); `sse.ts` and `http-client.ts` 100% function coverage maintained.
- **D38 — `src/lib/error-map-presets` + `src/lib/run-command` migration.** DONE.
  - Spec correction: D38 was originally framed as three files; `error-codes` already migrated in D34 as `src/constants/error-codes.ts`, so the actual scope is two files (120L + 185L) under `src/lib/`.
  - `error-map-presets.ts` exports a `PresetEntry` interface + `PresetMap = Readonly<Record<string, PresetEntry>>` alias; the three frozen preset records (`AUTH_PRESET`, `WORKSPACE_PRESET`, `ZOMBIE_PRESET`) and `compose(...sources)` are typed against it. `compose` rebuilds the accumulator explicitly so the return type stays `PresetMap` without a `Record<string,_>` widening cast.
  - `run-command.ts` types its full input surface: `RunCommandOptions { name, handler, retry, instrument, errorMap, ctx, deps }`, `RunCommandDeps { analyticsClient, distinctId, trackCliEvent, printJson, writeLine, ui }`, `HandlerCtx` (forward-declared with an index signature — the canonical `Deps` shape lands in D39 when commands consume it), and internal `RenderOpts`. `isFetchFailed` becomes a type-predicate; the catch block narrows `unknown` to `Error` via `instanceof` before reading `.message` (replaces the original `String(err?.message ?? err)` which TS rejected under `useUnknownInCatchVariables`). `noUncheckedIndexedAccess` forced `errorMap[err.code ?? ""]` + `remap?.code` / `remap?.message` discipline.
  - **`analytics.d.ts` drift fix (carry-forward risk #2 materialized):** the `getCliAnalyticsContext` declaration was wrong — `.d.ts` claimed `(distinctId: string) → CliAnalyticsContext`, runtime is `(ctx) → Record<string,unknown>` (reads `ctx.analyticsContext` and spreads). Corrected signature in-place. Four other declarations (`createCliAnalytics`, `cliAnalytics` shape, `drainCliAnalyticsEvents` param, the fictional `CliAnalyticsContext` interface) are pre-existing lies from D37 that D38 doesn't touch; deferred — they dissolve when `analytics.js` itself migrates and the `.d.ts` is deleted. Captured in `[[feedback_dts_drift_audit_at_migration_seam]]`-style spec note for the analytics-migration wave.
  - **String-vs-context decision on `getCliAnalyticsContext` shape:** considered three alternatives. `(distinctId: string)` would require a global registry keyed by `distinctId`, but `setCliAnalyticsContext` mutates `ctx.analyticsContext` in place — the data store is per-ctx, not per-id. Inlining the one-line body at the call site is cheaper still but nukes the `analyticsContext` field-name abstraction. Lowest-effort easy fix is the `.d.ts` signature correction; chose that.
  - 12 consumer imports updated `.js → .ts` via batch sed: `src/commands/{auth,billing,core,zombie,tenant,workspace,grant,agent,core-ops}.js`, `src/program/handlers-bind.js`, `test/{error-matrix,run-command}.unit.test.js`. oxlint's `import(extensions)` rule was the gate that surfaced the post-rename misses (typecheck alone resolves cross-extension and would have green-lit a stale import).
  - 684 pass + 2 skip + 0 fail across 62 files (matches pre-D38 baseline; rename + type-addition is behaviorally neutral). `bun run lint` (oxlint + audit-runtime-imports + tsc --noEmit) all clean across 139 files.
- **D39 — `src/commands/**` migration (each command).** DONE.
  - **Scope expansion (acknowledged during execution):** D39 also dragged in 4 small `.js` files that command modules import — `lib/api-paths.js` (13/14 consumers), `lib/load-skill-from-path.js`, `program/io.js`, `program/validate.js`. Migrating in-place (178L total) was cheaper than 4 new `.d.ts` bridges with drift risk (carry-forward risk #2). D40's program/ scope is correspondingly smaller — only `cli-tree`, `handlers-bind`, `help`, `banner`, `auth-guard` remain.
  - **`src/commands/types.ts` (NEW)** — shared shapes for every command: `CommandCtx` (extends `HandlerCtx` from D38's run-command with the streams + credential fields commands read), `ParsedArgs` (with `positionals` plural, matching runtime), `Workspaces`, `WorkspaceEntry`, `CommandDeps` (15 fields mirroring `buildDeps()` in cli.js), `CommandHandler` (canonical `(ctx, parsed, workspaces, deps) → Promise<number>` signature), `SpinnerOptions`/`SpinnerHandle`, `StreamGetFn`. Option readers `readString`/`readBoolean`/`readNumber` for the `parsed.options[key]` narrowing pattern (Captain-prompted — TS has no built-in `isString`/`isNumber`; project-specific narrowing helpers are the right level of abstraction).
  - **14 command files migrated:** `agent`, `auth`, `billing`, `core-ops`, `core`, `grant`, `tenant`, `workspace`, `zombie`, `zombie_credential`, `zombie_events`, `zombie_list`, `zombie_logs`, `zombie_steer` (`.js → .ts` via `git mv` history-preserving). `noUncheckedIndexedAccess` forced explicit `ctx.stdout`-presence guards; `useUnknownInCatchVariables` forced `err instanceof Error && typeof err.message === "string"` narrowing in catch blocks; `exactOptionalPropertyTypes` forced parameter widening on `SpinnerOptions.stream`, `openUrl(url, { env })`, and `CredentialFile.session_id`.
  - **`zombie.ts` FLL split:** typed version was 394L (cap 350) — `commandInstall` + `commandUpdate` extracted to `zombie_install.ts` (new, 198L) so both files stay under cap. **No re-export shim in `zombie.ts`** (carry-forward risk #5 — Captain's principle from D37): `handlers-bind.js` and `test/helpers.js` import `commandInstall`/`commandUpdate` from `zombie_install.ts` directly. The brief shim added during migration was removed immediately when discovered.
  - **`analytics.d.ts` `getCliAnalyticsContext` drift** (carried over from D38) — Captain pushed back on reflex-mirroring; took the lowest-effort fix (correct the `.d.ts` signature) after analyzing whether `(string)` or `(ctx)` was the right design (verdict: per-ctx storage means `(distinctId)` was never an option).
  - **Captain-spotted debt cleaned during migration:** (1) UZ-GRANT-003 leak in `grant.ts` operator stdout — replaced with human-readable description ("The zombie can no longer use this integration; further attempts will be denied"); test `grant.integration.test.js` updated. (2) Three repeated string literals — `TOKEN_EXPIRED`, `UNAUTHORIZED`, `REQUEST_FAILED` — promoted to named consts in `constants/cli-errors.ts`, plus `INVALID_ARGUMENT` for credential validation. Consumers updated (`error-map-presets.ts`, `auth.ts`, `core-ops.ts`).
  - **Verified end-state:** 684 pass + 2 skip + 0 fail across 62 files (unchanged from pre-D39 baseline). `bun run lint` clean across 141 files (oxlint + audit-runtime-imports + tsc --noEmit). `zombie.ts` at 236L, all other files under 350L.
- **D40 — `src/program/**` migration.** DONE.
  - **Surface clarification on entry:** the D-wave row listed 8 files (cli-tree, handlers-bind, help, io, validate, validators, banner, auth-guard). `io.ts` + `validate.ts` already migrated in D39 as drag-ins; D38's scope-narrowing note ("only cli-tree, handlers-bind, help, banner, auth-guard remain") under-counted by one — `validators.ts` was omitted. Discovery during D40 found a seventh sibling: `program/cli-tree-zombie.js` (zombie subtree extracted from `cli-tree.js` for the LENGTH GATE) had not been enumerated in any prior wave entry. Final D40 surface: 7 files = `auth-guard`, `banner`, `help`, `validators`, `cli-tree-zombie`, `cli-tree`, `handlers-bind`. All seven migrated via `git mv` (history preserved).
  - **`program/cli-tree-types.ts` (NEW)** — extracted from `cli-tree.ts` when the typed file hit 361L (over the 350L FLL cap by 11). Owns the structural types for the command tree: `ActionFrame`, `CommandHandlerFn`, the per-group handler interfaces (`AuthHandlers`/`WorkspaceHandlers`/`AgentHandlers`/`GrantHandlers`/`TenantHandlers`/`BillingHandlers`/`ZombieHandlers`/`ZombieCredentialHandlers`), `Handlers` (root composite), `ProgramState`, `BuildProgramOptions`, `ActionDispatch` (injected `actionFor` + `runHandler` pair). `cli-tree.ts`, `cli-tree-zombie.ts`, and `handlers-bind.ts` all import from this types module — **no re-export shim in `cli-tree.ts`** (carry-forward risk #5 honored).
  - **Captain-spotted dedup during migration (validate.ts → validators.ts merge):** the two modules carried a private `EXAMPLE_UUIDV7` literal and duplicated the uuidv7 check (`validate.ts:isValidId` handler-side type guard vs `validators.ts:parseIdOption` commander parser); near-identical filenames invited the "which one do I use?" confusion that produced the duplication. First pass extracted `isValidId` as the shared impl; Captain pushed back on the two-file split — one mental model, not two. Final shape: `validate.ts` deleted; `validators.ts` now owns both flavors (commander parsers that throw `InvalidArgumentError`, plus handler-side type guards / `ValidateResult` bag for paths that need to format failures into a `ui.err()` line without unwinding through commander). Combined module is 201L — well under cap. 6 command imports (`agent`, `grant`, `workspace`, `zombie`, `zombie_install`, `zombie_logs`) + `test/validate.test.js` redirected via batch sed. Test files kept split (88L + 272L = 360L would exceed FLL cap, and they exercise distinct paths cleanly).
  - **`analytics.d.ts` `cliAnalytics` drift** (carry-forward risk #2 materialized at the D40 seam): the `.d.ts` declared `cliAnalytics` as a function (`(env?) => Promise<AnalyticsClient | null>`), but the real export is a namespace object containing `createCliAnalytics` / `trackCliEvent` / `trackHttpRequest` / `trackHttpRetry` / `shutdownCliAnalytics` (analytics.js:138–143). Surfaced at `handlers-bind.ts:97` (`cliAnalytics.trackCliEvent` failed TS2339). Fixed the declaration to mirror the literal — `cliAnalytics` is now `const cliAnalytics: { ...readonly methods }`. Also added the missing `shutdownCliAnalytics` declaration (used by `cli.js`, will trip D41 typecheck otherwise). Three other `.d.ts` drifts remain (`createCliAnalytics` shape, `drainCliAnalyticsEvents` param, fictional `CliAnalyticsContext`) — they dissolve when `analytics.js` itself migrates.
  - **Typed `Handlers` tree:** the nested handler map `cli.js` assembles is now strongly typed as `Handlers` (from `cli-tree-types.ts`). Every leaf is `CommandHandlerFn = (frame: ActionFrame) => Promise<number | void> | number | void`. The wrap-and-bind path in `handlers-bind.ts` returns `Handlers`; `buildProgram({ handlers: Handlers, ... })` accepts it. `cli-tree-zombie.ts` accepts `Handlers` and uses `handlers.zombie` slice — no narrower type needed since the tree is a single shape.
  - **Lifecycle interface:** `handlers-bind.ts` exports `Lifecycle { ctx: CommandCtx; workspaces: Workspaces; deps: CommandDeps; analyticsClient: AnalyticsClient | null | undefined; distinctId: string; lastCommand: string | null }`. `buildHandlers(lifecycle: Lifecycle): Handlers`. The `lastCommand: string | null` slot is mutated after each wrap call (commander callback contract). D41 will tighten when `cli.js` migrates and the lifecycle constructor moves into TS.
  - **`exactOptionalPropertyTypes` discipline in D40:** all optional fields on the new option-bag interfaces (`PrintVersionOptions`, `PrintPreReleaseWarningOptions`, `ZombieHelpOptions`, `IntBounds`, `PathOptions`, `JsonObjectOptions`, `BuildProgramOptions.helpFactory`) explicitly include `| undefined` so callers can pass `ctx.field` whose static type already includes `undefined`. Carry-forward risk #3 honored.
  - **8 consumer imports updated** `.js → .ts` via batch sed: `src/cli.js`, `test/auth-guard.test.js`, `test/banner.unit.test.js`, `test/cli-tree.parse.unit.test.js`, `test/help.unit.test.js`, `test/helpers-cli-tree.js`, `test/json-contract.test.js`, `test/validators.unit.test.js`. Plus `test/output-invariants.unit.test.js` fixture path bumped (was reading `banner.js` in the invariant-walk; now `banner.ts`).
  - **Verified end-state:** 684 pass + 2 skip + 0 fail across 62 files (matches pre-D40 baseline; rename + type-addition is behaviorally neutral). `bun run lint` clean across 142 files (oxlint + audit-runtime-imports + tsc --noEmit). File LOC: cli-tree.ts 280, handlers-bind.ts 166, validators.ts 155, cli-tree-zombie.ts 128, cli-tree-types.ts 113, banner.ts 75, help.ts 51, auth-guard.ts 21 — every file well under cap.
- **D41 — `src/cli.{ts}` + entry-point drag-ins + type-system unification.** DONE.
  - **Surface clarification on entry:** the D-wave row listed `cli.js + bin/zombiectl.js`. Migration of the bin shim deferred — `bin/zombiectl.js` is the published Node entry (`package.json` `bin` field), and Node can't execute `.ts` directly. D43 emits compiled `.js` to `dist/` and rewrites the bin path; migrating the 11-line shim to `.ts` now would break `npm install -g` until D43 lands AND double the bin-path churn. The shim's import of `runCli` updated `.js → .ts`, but the file stays `.js`. Two drag-ins folded in: `src/lib/browser.js` (96L, runtime import from cli.js) + `src/ui-progress.js` (67L, spinner factory consumed by cli.js's `buildDeps`). Pattern matches D39 (4 drag-ins / 178L) — migrating in-place avoids `.d.ts` bridges with drift risk (carry-forward risk #2).
  - **Final D41 surface (3 files migrated + 4 type-system fixes):** `src/cli.ts` (280L typed dispatcher), `src/lib/browser.ts` (123L, with `BrowserResolution` discriminated union), `src/ui-progress.ts` (78L, returns canonical `SpinnerHandle`). Plus: `bin/zombiectl.js` import path bump.
  - **Type-system unification forced by the migration (Captain-confirmed scope):**
    - **Duplicate `Workspaces` interface eliminated** — `commands/types.ts` had its own `Workspaces` (open shape with fictional `workspaces?` + `current_workspace_label?` fields) that drifted from the actual on-disk shape in `lib/state.ts`. Once `cli.ts` was typed, the assignment `lifecycle.workspaces = await loadWorkspaces()` failed with `Workspaces is not assignable to Workspaces` (cross-file structural mismatch). Resolved by re-exporting `Workspaces` and `WorkspaceItem` from `lib/state.ts` via `commands/types.ts` (single source of truth, owned by the file that defines the on-disk shape). Knock-on: `workspace.ts` dropped its private `WorkspaceListEntry` + `WorkspacesWithItems` + `ensureItemsArray` helper (`items` now lives in the base type; the lazy-init helper was defensive code against a phantom missing-field path).
    - **`CredentialFile` type retired** — was a permissive placeholder added in D39 (`token?, saved_at?, session_id?, [key]: unknown`); the real on-disk shape is `Credentials` from `lib/state.ts` (`token`/`saved_at`/`session_id`/`api_url`, all `string|null`). Replaced everywhere — `auth.ts:192` drops the `as CredentialFile | null` cast; `core.ts:225` drops the `as { ...api_url?: string }` cast on the saveCredentials literal. CommandDeps.loadCredentials/saveCredentials now use `Credentials`.
    - **`CommandCtx.apiUrl` tightened required** — was optional (`apiUrl?: string`), but `cli.ts` always sets it before any handler runs AND `http-client.ts:HttpRequestContext.apiUrl` is required. Optional on the source side caused parameter-contravariance failures when assigning `apiHeaders`/`request` into `CommandDeps`. Tightening to required matches reality.
    - **`Lifecycle.distinctId` widened to `string | null`** — `extractDistinctIdFromToken(token)` returns `null` when there's no auth token, and `run-command.ts` applies the `"anonymous"` fallback at the analytics-emit boundary (`deps.distinctId ?? handlerCtx.distinctId ?? "anonymous"`). The first migration pass coerced `distinctId` to empty string via `?? ""`, which silently broke the fallback (`"" ?? "anonymous"` → `""`, not "anonymous", because `??` only catches nullish). The failing test `runCli tracks login success with post-login distinct id and shuts down analytics` caught this regression. Honest type accepts the null, and `handlers-bind.ts` translates to `undefined` at the run-command seam.
  - **`analytics.d.ts` drift sweep #2** (carry-forward risk #2 surfaced again at the cli.ts seam):
    - `drainCliAnalyticsEvents` declaration was a double lie — claimed `(client: AnalyticsClient | null) => Promise<void>`, real impl is `(ctx) => QueuedAnalyticsEvent[]` (synchronous, returns array). Fixed param to `unknown` (the real impl guards via `Array.isArray(ctx.analyticsEvents)`) and return type to `QueuedAnalyticsEvent[]`.
    - `trackCliEvent` distinctId param widened to `string | null | undefined` — matches the `distinctId || "anonymous"` fallback in `analytics.js:55`. cli.ts's `cliAnalytics.trackCliEvent(client, distinctId, ...)` now passes `string | null` honestly.
  - **`SpinnerOptions` + `SpinnerHandle` tightened** — `SpinnerOptions.style?: string` added (consumed by `ui-progress.ts:spinnerFrames` for "dotmatrix"/"matrix" alternate). `SpinnerHandle.succeed` / `fail` made required (impl always returns them; the optional `?` was defensive); both accept `message?: string` since the impl signature widened during typing.
  - **`CommandDeps.createSpinner` signature corrected** — was `(SpinnerOptions | string) => SpinnerHandle`; the `| string` form was never implemented in `ui-progress.ts`. Narrowed to `(SpinnerOptions) => SpinnerHandle`. No consumer passed a bare string (core.ts is the only call site; passes an option object).
  - **`RunCommandDeps` optional fields explicit-undefined-ified** — under `exactOptionalPropertyTypes: true`, `analyticsClient?: T | null` means "may be omitted OR be `T | null`" but NOT `undefined`-by-value. `handlers-bind.ts`'s `wrapHandler` builds the deps object with `lifecycle.distinctId ?? undefined`, which is explicitly `string | undefined`. Widened each field to `T | undefined` so the assignment passes. Same pattern as D39 [[feedback]] (`AuthCredentials.{token,apiKey}` etc).
  - **`exactOptionalPropertyTypes` discipline in D41:** every option-bag exposed by the new files (`PrintVersion`-class interfaces were already done in D40; D41 adds `RunCliIo`, `OpenUrlOptions`, `BrowserResolution{Ok,Blocked}`) explicitly includes `| undefined` for optional fields.
  - **Captain-spotted bin scope adjustment:** original HANDOFF text said "bin stays minimal shim" — confirmed interpretation: shim STAYS `.js`. The Discovery prose above documents why migrating it now would be churn (D43 replaces it anyway).
  - **18 consumer imports updated via batch sed** — `bin/zombiectl.js`, plus 17 test files (auth-guard, banner, cli-tree.parse, json-contract, output-invariants, cli-alignment, cli-analytics, doctor-json, etc.) and 2 production callers (`src/lib/run-command.ts` + `src/program/cli-tree.ts` comment refs to `cli.js → cli.ts`). The sed pass over-reached on `acceptance/fixtures/cli.{js,ts}` (sibling spawner stays `.js`); reverted with a targeted second pass.
  - **Audit-script regex collision** — wrote a documentation comment that contained the literal text `cross-import "Type 'Workspaces'..."`; `scripts/audit-runtime-imports.mjs`'s naive `import\s+...["']...["']` regex doesn't strip comments and matched `import "Type "` as a bogus dependency. Rephrased the comment (dropped the embedded double-quoted string). [[feedback_blast_radius_grep]]-adjacent — naive regex tools collide with prose that mentions language keywords.
  - **Verified end-state:** 684 pass + 2 skip + 0 fail across 62 files (matches pre-D41 baseline; rename + type-addition + type-system unification are all behaviorally neutral). `bun run lint` clean across 141 files (oxlint + audit-runtime-imports + tsc --noEmit). File LOC: cli.ts 280, browser.ts 123, ui-progress.ts 78 — all well under cap.
- **D42 — `test/**` migration.** IN_PROGRESS. Batched per-domain to keep commits reviewable; ~79 `.js` test files at D41 end-state, reduced as each batch lands.
  - **D42a — `test/acceptance/**` (18 files).** DONE (commit `83c0bfa5`). Spec + fixture migration; `acceptance/fixtures/cli.js` deliberately stays `.js` (Node-spec spawner; D43 switches it back to `node` against compiled output — carry-forward risk #5).
  - **D42-fixup — merge fallout from `origin/main` reconciliation.** DONE this batch. `7114247a` brought ~120 files from main (M70 UFS/PUB refactors, `audit-const-names.mjs`, `coverage-fill-patch.unit.test.js`); three latent regressions surfaced once the gates ran scoped-clean:
    - **`test/coverage-fill-patch.unit.test.js → .ts`** — main's PR-#325 patch-coverage test was authored against a pre-D39 surface (`commandInstall` from `commands/zombie.js`; D39 moved it to `commands/zombie_install.ts`) and imported `.js` extensions where D34/D40 had migrated targets to `.ts`. Rewrote with proper `CommandCtx`/`CommandDeps`/`ParsedArgs` types, `Writable` mock streams matching the tightened `NodeJS.WritableStream` shape, and a `ResponseLike → FetchImpl` widening helper. Set the pattern D42b–d unit tests will follow for fetch-mock and partial-deps-mock surgery. 18 new tests now passing.
    - **`src/program/cli-tree.ts:111` promise(always-return)** — `Promise.resolve(handler(frame)).then((code) => { state.exitCode = ... })` mutated state without returning. Converted to `async/await` (now `const code = await handler(frame); state.exitCode = ...`). Single 4-line change; semantics preserved.
    - **`src/program/cli-tree-consts.js` orphan deletion** — main's M70 UFS sweep extracted ~19 `K_*` string constants from main's `.js cli-tree` for FLL relief. Our branch's D40 `cli-tree.ts` was authored earlier and never imports any of them; the file landed with zero consumers across the worktree. Per RULE NDC (no dead code at write time), deleted rather than `.ts`-migrated. The harness `audit-const-names.mjs` walks `.js`/`.mjs` only, so dropping the file is invisible to it.
    - **`test/helpers.d.ts`** — added ambient declarations for `test/helpers.js` (still `.js` until D42c). Mirrors the `src/lib/analytics.d.ts` pattern (carry-forward risk #2). Declares `buildParsed`, `makeNoop`, `makeBufferStream`, `createCoreHandlers`, `commandTenant`, `commandBilling`, `commandZombieDispatch`, `ui`, and the stable test ID constants. D42c will collapse this into `helpers.ts` when the file itself migrates.
    - **Verified end-state:** 702 pass + 2 skip + 0 fail across 63 files; lint 0/0; typecheck clean.
  - **D42b — `test/*integration*.test.js` cluster (~10 files).** Pending.
  - **D42c — `test/*.unit.test.js` cluster + `test/helpers.{js → ts}` (~31 files).** Pending. helpers migration dissolves `helpers.d.ts`.
  - **D42d — remaining `test/*.test.js` misc (~20 files).** Pending.
- **D43 — Build pipeline: tsc emit to `dist/`, update `bin` + `files` in `package.json`, switch acceptance spawner to compiled output for global mode.** Pending. After this lands, the published artifact is pure `.js`; `audit-runtime-imports.mjs` can be narrowed back to `.js`-only walk.

Out of M68 (handoff to follow-up): converting `zombied/scripts/**/*.mjs` (Zig-tree side scripts) and the e2e harness at `ui/packages/app/tests/e2e/cli-acceptance/`. The Zig side is unaffected — only the JS surface migrates.

**§14 Risks & Gotchas (carry into D38–D43):**

- **`bun test` now picks up `*.spec.*` files.** The 5 acceptance specs in `test/acceptance/` run on every `bun test` invocation (~7s overhead — stub-mode without `ZOMBIE_ACCEPTANCE_TARGET` yields 51 pass + 2 skip). If overhead becomes a problem, rename the specs or narrow the `bun test` glob. `coveragePathIgnorePatterns` was widened from `test/acceptance/**` to exclude acceptance from coverage math — keep that exclusion when adding new acceptance suites.
- **Ambient `.d.ts` files lie if they drift.** D37 introduced `src/lib/analytics.d.ts` as the TS→JS bridge for the still-JS `analytics.js`. The compiler trusts the `.d.ts` blindly; if `analytics.js` adds/removes/renames an export the `.d.ts` MUST be updated in the same commit. When the JS file migrates (D38+), delete its `.d.ts`. Do not keep both. Same pattern applies when D38 introduces a `.d.ts` for `lib/run-command.js` if consumers need typed imports before the file itself migrates.
- **`exactOptionalPropertyTypes` widens parameter types, not field types.** Parameter shapes that accept caller-produced values should be `string | null | undefined` (not `string | null`) so callers can pass already-`undefined`-typed expressions without an extra guard at every call site. Tighten at the boundary if a narrower internal type makes sense (e.g. `apiRequestWithRetry` converts `null` to `{}` internally). Pattern baked into `AuthCredentials.{token,apiKey}`, `ApiRequestWithRetryOptions.retry`, `fetchImpl?` — propagate to D38–D41 wherever a parameter shape repeats the trap.
- **`src/lib/http.ts` is at 327 / 350 lines — 23 lines of FLL headroom.** D38–D43 must NOT add code to `http.ts`. Net-new transport code belongs in `stream-fetch.ts` (POST/SSE) or `sse.ts` (GET/SSE) or a new sibling module. The split happened because Captain rejected trimming in place when the gate fired at 412L; respect the verdict.
- **No re-export shims.** When D37 carved `streamFetch` out of `http.js` into `stream-fetch.ts`, Captain explicitly rejected adding a re-export from `http.ts`. Callers import from the owning module directly. The principle: each module is one honest surface, not a façade hiding a split. Apply to any future surface decomposition in D38–D43 — if D39 splits a command, callers import from the new file, not via a shim in the old one.
- **Acceptance fixture spawner uses `bun`, not `node`, until D43.** `test/acceptance/fixtures/cli.js` invokes `bun` to run the CLI because the source tree mixes `.js` and `.ts` and only Bun resolves cross-extension imports at runtime. After D43 emits compiled `.js` to `dist/`, the spawner switches back to `node` against the compiled output, and `scripts/audit-runtime-imports.mjs` can narrow its walk back to `.js`-only. Do not "fix" the `bun` spawn before D43 lands.

---

## Interfaces

### HTTP (additive only — no breaking change)

```
POST /v1/workspaces/{ws}/zombies
  body:  { trigger_markdown: string, source_markdown: string }    // unchanged
  201:   {
    zombie_id: string,
    name: string,
    status: string,
    webhook_urls: { [source: string]: string }                    // NEW; empty {} when no webhook triggers
  }

GET /v1/workspaces/{ws}/zombies
  200:   {
    items: [{
      id, name, status, created_at, updated_at,                   // unchanged
      triggers: ?[{                                               // NEW
        type: "webhook" | "cron" | "api",
        source?: string,                                          // webhook only
        events?: string[],                                        // webhook only
        schedule?: string                                         // cron only
      }]
    }],
    total, cursor
  }

GET /v1/workspaces/{ws}/zombies/{id}/events?actor_prefix=<prefix>&limit=<n>
  200:   { items: [...], next_cursor }                            // optional new query param
  Note:  actor_prefix is additive; absent → today's behaviour.

PATCH /v1/workspaces/{ws}/zombies/{id}                            // §10b — body fields ADDITIVE
  body: {                                                         // all fields optional, ≥1 required for non-noop
    status?:           "active" | "stopped" | "killed",           // unchanged
    config_json?:      string,                                    // unchanged (whole-blob replace)
    trigger_markdown?: string,                                    // NEW — reparses, overlays triggers half
    source_markdown?:  string                                     // NEW — reparses, overlays tools/creds/network/budget half
  }
  200:   { zombie_id, status?, config_revision }
  409:   { error_code: "UZ-ZMB-010", ... }                        // FSM transition rejected (unchanged)
  503:   { error_code: "UZ-DB-XXX", ... retryable }               // NEW — lock_timeout
```

### Frontmatter (TRIGGER.md)

```yaml
x-usezombie:
  triggers:                                              # ARRAY required; 1–8 entries
    - type: webhook
      source: github
      events: ["workflow_run"]                           # NEW field; 1–16 elements
      signature: { secret_ref, header, prefix, ts_header? }
    - type: cron
      schedule: "*/30 * * * *"
    - type: api                                          # generic JSON ingress
  tools: [...]
  credentials: [...]
  network: { allow: [...] }
  budget: { daily_dollars, monthly_dollars? }
```

### CLI

```
zombiectl install --from <path> [--json]
  Now prints webhook_urls map per trigger in JSON mode.
  No change to non-JSON output other than one trailing line listing the webhook URLs.

zombiectl zombie update <id> --from <dir> [--json]                       // NEW (§10b)
  Reads TRIGGER.md + SKILL.md from <dir>, PATCHes /zombies/{id} with
  {trigger_markdown, source_markdown}. JSON mode prints { config_revision }.
  No concurrency-token flag — LWW; row-lock + field-merge handles same-millisecond races.

(Trigger registration still runs in the install-skill via gh, not via a new
 zombiectl subcommand — `zombie update` is for update-in-place after initial install.)
```

### Vault credential shapes (unchanged from M45)

```
github   = { api_token: "...", webhook_secret: "..." }
linear   = { api_token: "...", webhook_secret: "..." }
jira     = { api_token: "...", email: "...", webhook_secret: "..." }
grafana  = { api_token: "..." }
slack    = { signing_secret: "..." } | { bot_token: "..." } | { incoming_webhook_url: "..." }
agentmail = { api_key: "..." }
```

### `@assistant-ui/react` integration

```typescript
const runtime = useExternalStoreRuntime<ZombieMessage>({
  messages,                                              // ZombieMessage[] from useZombieEventStream
  isRunning,
  onNew: async (msg) => steerZombie(ws, zid, msg.content[0].text, token),
});
```

`ZombieMessage` extends `Message` with `customData: { actor, type, request_json, event_id, created_at_ms }`.

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| TRIGGER.md singular `trigger:` | Legacy fixture or pre-spec author | 400 `ERR_ZOMBIE_INVALID_CONFIG` with message `use "triggers:" (array)` |
| Empty `triggers: []` | Author error | 400 with message `triggers must contain at least one entry` |
| Two `webhook:github` entries | Author error | 400 with message `duplicate trigger (type, source) tuple` |
| `triggers` length > 8 | Adversarial / mistake | 400 with message `max 8 triggers per zombie` |
| `events` length > 16 | Same | 400 |
| Install with no webhook trigger | Cron-only / api-only zombie | 201 with `webhook_urls: {}`. CLI install-skill skips S1.9; smoke-test steer (S1.11) is the validation path. |
| `gh auth` lacks `admin:repo_hook` | Stale token | Skill prints `gh auth refresh -s admin:repo_hook` and stops at S1.0 |
| `gh api` 422 hook-exists | Re-install on same repo | Skill GETs hooks list, matches `config.url`, treats as registered, advances |
| `gh api` 404 | Repo / token wrong | Skill prints response verbatim, stops |
| `gh api` 5xx | GitHub transient | Skill retries once with 2s backoff, stops on second 5xx |
| Assistant-ui SSE drop | Network blip | Hook closes EventSource, polls `listZombieEvents(since=last_event_id)` for backfill, reopens SSE |
| Steer fails (500) | Backend transient | Optimistic frame renders red with retry button; SSE state unchanged |
| Steer fails (409 — agent busy) | Prior stage still running | Composer disabled with caption "agent is responding…" |
| Bundle exceeds 220 kB gz | Tree-shake regression | CI bundle-budget check fails; implementer reduces footprint before merge |
| Free-trial end timestamp passed in test fixture | Time-based test brittleness | Tests inject `now_ms` via dependency injection; no live-clock dependence |
| Row-lock wait > 5s (§10b) | Another writer holding `FOR UPDATE` for >5s (unexpected — pathological client, blocked I/O) | `lock_timeout` fires; Postgres returns `55P03`; handler maps to `503 ERR_DB_LOCK_TIMEOUT` (retryable). Pool connection released, not leaked. |
| Concurrent PATCH `{trigger_markdown}` + `{source_markdown}` on same zombie (§10b) | Two operators / one operator + CLI script | Both succeed via row-lock merge; final `config_json` reflects both halves (no silent clobber) |
| Concurrent PATCH on same field (§10b) | Same operator double-click or two CLI invocations | Last-write-wins; both responses succeed; no deadlock, no error |
| Concurrent PATCH + DELETE on same zombie (§10b) | Operator A patches config while operator B deletes a just-killed zombie | Postgres serializes via row-lock; whichever entered first commits; second observes new state (PATCH on deleted → 404; DELETE on un-killed → 409) |
| Worker reload races with PATCH (§10a / §10b) | PATCH commits while worker is mid-reload of a prior `config_changed` | Worker uses plain SELECT (MVCC); no lock contention; reload converges to the latest `updated_at` it observes |
| Webhook secret rotation race (out of M68; flagged) | Upstream signs payload with `S1`; user rotates to `S2`; webhook arrives at receiver | Receiver verifies against `S2` → HMAC mismatch → 401 → upstream redelivers. Fix is dual-secret window (`webhook_secret_previous` in vault, receiver tries both); separate future spec — out of M68 scope, documented here so a reader of §10b's concurrency discussion doesn't think we missed it. |

---

## Invariants

1. **Receiver substrate unchanged.** M43_001's `/v1/webhooks/{id}/{source}` accepts the same payloads with the same HMAC verification. Zero edits to `src/http/handlers/webhooks/*.zig` or `src/auth/middleware/webhook_sig.zig`.
2. **One event-processing path.** The worker (`src/zombie/event_loop.zig`) never branches on actor type. The `triggers` array is config-only; the runtime keeps single-stream ingress per `data_flow.md` §B.
3. **Actor field is prompt-side load-bearing, platform-side opaque.** SKILL.md prose is the discriminator between `steer:howdy` and `webhook:github` — confirmed by snapshot tests against fixture envelopes.
4. **No new database schema.** `config_json` is jsonb; new fields project via JSON path operators.
5. **No new endpoint.** Three additive response fields (`webhook_urls`, `triggers`, optional `actor_prefix` query) on existing endpoints.
6. **Cross-tier rate constant parity.** `STAGE_PLATFORM_NANOS`, `STAGE_SELF_MANAGED_NANOS`, `FREE_TRIAL_STAGE_NANOS`, `FREE_TRIAL_END_MS` carry identical names + numeric values across `src/state/tenant_billing.zig`, `ui/packages/website/src/lib/rates.ts`, `ui/packages/app/lib/types.ts`, `zombiectl/src/constants/billing.js`. Paired pin tests fail if any side drifts.
7. **No "Mission Control" string survives.** Lead-repo `grep -rln "Mission Control" .` (excluding `node_modules`, `.next`, `.zig-cache`) returns empty before lead-PR open. Companion-PR's `grep -rln "Mission Control" ~/Projects/docs/` returns empty before companion-PR open. The two greps are independent invariants — each PR's grep must show empty in its own scope.
8. **No singular `trigger:` survives.** `grep -rln "^\s*trigger:" samples/ src/ tests/` (modulo legitimate non-frontmatter uses) returns zero; positive cases captured in PR Discovery.
9. **Free-trial path is timestamp-gated, not flag-gated.** `is_free_trial_active(now_ms)` reads `FREE_TRIAL_END_MS`. No environment variable, no feature flag, no database column.
10. **§10b txn locks exactly one row.** `patch.zig`'s body-field transaction performs `SELECT … FOR UPDATE` on a single `core.zombies` row and one `UPDATE` on that same row. No second `SELECT FOR UPDATE`, no second locked table, no second row. Deadlock is structurally impossible. Future edits adding a second lock acquisition in this txn must re-run deadlock analysis and add coverage to `patch_concurrent_integration_test.zig`.

---

## Test Specification

### Backend (Zig)

| Test | File | Asserts |
|---|---|---|
| `test_triggers_array_required` | `src/zombie/config_test.zig` | `triggers: []` → 400 `ERR_ZOMBIE_INVALID_CONFIG` |
| `test_triggers_singular_rejected` | same | Singular `trigger:` → 400 with the precise hint message |
| `test_triggers_max_count` | same | 9 entries → 400 |
| `test_triggers_dedup_source` | same | Two `webhook:github` → 400 |
| `test_triggers_one_cron` | same | Two `cron` → 400 |
| `test_events_array_caps` | same | 17 events → 400; element > 64 chars → 400 |
| `test_events_parsed_correctly` | same | Valid envelope → array roundtrip preserves order |
| `test_install_response_webhook_urls` | `src/http/handlers/zombies/api_integration_test.zig` | Install with `webhook:github` trigger → 201 includes `webhook_urls.github = "${ORIGIN}/v1/webhooks/${id}/github"` |
| `test_install_response_no_webhook` | same | Install with cron-only triggers → `webhook_urls: {}` |
| `test_list_projects_triggers` | same | List returns `triggers` array projection on each row |
| `test_free_trial_stage_returns_zero` | `src/state/tenant_billing_test.zig` | `compute_stage_charge(now_ms=before_trial_end)` returns 0 |
| `test_post_trial_stage_returns_rate` | same | `compute_stage_charge(now_ms=after_trial_end)` returns `STAGE_PLATFORM_NANOS` |
| `test_rates_pinned_cross_tier` | same | Zig constants match TypeScript constants match JavaScript constants — assertion on string-equal serialisation |
| `test_merge_overlay_trigger_only` | `src/http/handlers/zombies/patch_merge_test.zig` | Overlay `{trigger_markdown}` onto current config_json — only `x_usezombie.triggers` replaced; tools/credentials/source untouched |
| `test_merge_overlay_source_only` | same | Overlay `{source_markdown}` — only tools/credentials/network/budget replaced; triggers untouched |
| `test_merge_overlay_both` | same | Both fields together — both halves replaced in one merged jsonb |
| `test_merge_reparse_error_rolls_back` | same | Malformed `trigger_markdown` reparse fails → caller receives error, txn ROLLBACKs (verified via unlock + next-writer-succeeds in integration) |
| `test_patch_trigger_md_only_persists_and_publishes` | `src/http/handlers/zombies/patch_body_fields_integration_test.zig` | `{trigger_markdown}`-only PATCH reparses, persists merged config_json, XADDs `config_changed` once |
| `test_patch_source_md_only_persists_and_publishes` | same | `{source_markdown}`-only same |
| `test_patch_both_fields_one_update` | same | Both fields → one SQL UPDATE; one config_changed XADD; one durable §10a system event row with new revision |
| `test_patch_reparse_rollback_releases_lock` | same | Malformed reparse mid-txn → ROLLBACK → next PATCH on same zombie succeeds within 1s (lock released, not leaked) |
| `test_patch_worker_reload_emits_system_row` | same | After §10b PATCH commits, worker observes `config_changed`, reloads, emits `cfg-{new_revision}` system event row (end-to-end through §10a path) |
| `test_concurrent_different_fields_both_land` | `src/http/handlers/zombies/patch_concurrent_integration_test.zig` | Thread A `{trigger_md}` + Thread B `{source_md}` released via barrier → both 200; final config_json contains both halves; **no `40P01` in either thread's response** |
| `test_concurrent_same_field_lww` | same | Thread A `{trigger_md=T2}` + Thread B `{trigger_md=T3}` → both 200; final value = whichever committed second; no deadlock_detected error |
| `test_concurrent_n_writers_no_pool_exhaustion` | same | N=10 PATCHes via thread pool on same zombie → all 10 succeed within 10s; pool connection count returns to baseline after |
| `test_concurrent_patch_delete_no_deadlock` | same | Thread A: PATCH; Thread B: DELETE same zombie → exactly one final state (PATCH-then-DELETE or DELETE-then-404); **assert no `40P01` deadlock_detected error code in either response body or log** |
| `test_concurrent_patch_webhook_insert_serializes` | same | Thread A: PATCH; Thread B: INSERT into `core.zombie_events` (FK ref to zombie) → both succeed; insert serializes after PATCH commit (latency proves wait) |
| `test_concurrent_patch_different_zombies_parallel` | same | Two PATCHes on distinct zombies → wall time < 1.5× single-PATCH (proves no false contention) |
| `test_lock_timeout_fails_fast` | same | Inject 7s `pg_sleep` via test fixture holding row lock → second PATCH returns `503 ERR_DB_LOCK_TIMEOUT` within 5.5s (not hung) |

### CLI install-skill (eval fixtures)

| Test | Asserts |
|---|---|
| `test_skill_s1_0_precondition_passes` | Mock environment with zombiectl + gh + auth → S1.0 advances |
| `test_skill_s1_0_missing_zombiectl` | Skill prints `npm install -g @usezombie/zombiectl`, stops |
| `test_skill_s1_0_missing_gh_scope` | Skill prints `gh auth refresh -s admin:repo_hook`, stops |
| `test_skill_s1_8_parses_triggers` | Rendered `TRIGGER.md` with `triggers: [github, cron]` → skill captures both, loops S1.9 only on webhook trigger |
| `test_skill_s1_9_gh_api_invocation` | Mock `gh` records command; assert matches template with substituted URL, events, secret reference |
| `test_skill_s1_9_422_idempotent` | Mock `gh` returns 422 hook-exists → skill GETs hooks, matches URL, advances |
| `test_skill_s1_9_403_scope_recovery` | Mock `gh` returns 403 → skill prints refresh command, stops |
| `test_skill_s1_10_hmac_self_verify` | Skill computes HMAC over canned payload, curls receiver, asserts 202 |

### Frontend (TypeScript / Vitest + Playwright)

| Test | File | Asserts |
|---|---|---|
| `test_provider_guidance_github` | `provider-guidance.test.ts` | Snapshot rendered command matches fixture |
| `test_provider_guidance_linear` | same | Snapshot matches Linear-specific curl with GraphQL body |
| `test_provider_guidance_jira` | same | Snapshot matches Jira REST with `-u email:token` and JQL |
| `test_provider_guidance_grafana` | same | Snapshot matches Contact-Point creation curl |
| `test_provider_guidance_slack` | same | Renders web-UI deep link + checklist (no curl) |
| `test_provider_guidance_agentmail` | same | Snapshot matches agentmail curl |
| `test_trigger_panel_renders_one_card_per_trigger` | `TriggerPanel.test.ts` | 3-trigger zombie → 3 cards in order |
| `test_trigger_panel_unknown_source_falls_back` | same | `source: "weirdco"` → `CopyUrlCard` rendered |
| `test_trigger_panel_last_delivery_line` | same | Mock event with `actor=webhook:github` → "Last delivery: webhook:github · <ts> · processed" line shows |
| `test_zombie_thread_optimistic_steer` | `ZombieThread.test.ts` | Submit → optimistic frame appears before SSE confirm |
| `test_zombie_thread_webhook_renders_as_chip` | same | Inject webhook envelope → renders as system chip with collapsible payload |
| `test_zombie_thread_tool_call_triple_collapses` | same | `tool_call_started` + `tool_call_progress` + `tool_call_completed` → single tool-call card mutates |
| `test_zombie_thread_steer_busy_disables_composer` | same | Inject `received` event with no `completed_at` → composer disabled |
| `test_pricing_renders_strikethrough` | `Pricing.test.tsx` | Both stage-rate lines wrapped in `<s>` element |
| `test_pricing_free_until_banner` | same | Banner copy contains "Free until July 31, 2026" verbatim |
| `test_onboarding_flow_four_cards` | `OnboardingFlow.test.tsx` | 4 numbered cards rendered in order |
| `test_onboarding_flow_snippets_match` | same | Snippet text matches the four canonical strings |

### Acceptance (Playwright)

| Test | Asserts |
|---|---|
| `acceptance/install-and-steer-via-ui` | Paste `TRIGGER.md` + `SKILL.md` → land on `/zombies/{id}` → see TriggerPanel + ZombieThread → type "howdy" → see optimistic bubble → see assistant reply stream |
| `acceptance/trigger-panel-github-card` | Install zombie with `source: github` → TriggerPanel renders GuidedTriggerCard with `gh api` block, URL is correct, snippet is copyable |
| `acceptance/onboarding-flow-renders-on-home` | Visit `/` → scroll to Pricing → scroll one section down → see four-card OnboardingFlow |

---

## Acceptance Criteria

1. CLI: a fresh machine running `npm install -g @usezombie/zombiectl` + `npx skills add usezombie/usezombie` + `zombiectl auth login` + `gh auth login -s admin:repo_hook` can complete `/usezombie-install-platform-ops` end-to-end with zero manual paste into github.com. The webhook self-verifies; the smoke-test steer round-trips.
2. Dashboard: pasting `TRIGGER.md` + `SKILL.md` at `/zombies/new` lands the user on `/zombies/{id}` where (a) TriggerPanel renders a per-trigger card list, (b) the GitHub card shows the pre-rendered `gh api` command ready to copy, (c) the ZombieThread chat surface shows webhook / cron events as system chips and the agent's reasoning as streaming assistant bubbles, (d) the composer at the bottom sends a steer that produces an optimistic bubble immediately and a real reply when the agent finishes.
3. Multi-trigger: a zombie declaring `triggers: [webhook:github, cron]` installs successfully; TriggerPanel renders one `GuidedTriggerCard` + one `CronCard`; the cron schedule is visible.
4. Pricing: the website's Pricing section renders both stage rates with strike-through plus "Free until July 31, 2026" banner. The page-level CTA reads "→ try free · free until July 2026". OnboardingFlow renders directly under Pricing.
5. Constants: `compute_stage_charge` returns 0 nanos when `now_ms < FREE_TRIAL_END_MS` and the existing rate after. Cross-tier identifier parity holds.
6. README badges: lead-repo `README.md` + `zombiectl/README.md` carry "Try for free" badges; no `$5` reference remains in either.
7. Mission Control: `grep -rln "Mission Control" ui/ ~/Projects/docs/` returns empty.
8. Architecture: `docs/architecture/user_flow.md` §8.2–§8.5, `data_flow.md` §B Trigger, and `scenarios/01_default_install.md` reflect the new DX flow; no stale "paste into GitHub" prose remains.
9. Companion `~/Projects/docs/` PR opens on branch `chore/m68-trigger-dx-free-trial` containing the rates snippet update, the pricing-prose deletions, the Mission Control rename, and the rewritten install + webhooks + quickstart pages.
10. Test suites: `make test` + `make test-integration` green; `make lint` clean; `make memleak` clean; `make check-pg-drain` clean; UI `bun test` + Playwright acceptance green; bundle for `/zombies/[id]` ≤ `pre-change measured baseline + 100 kB gz`; baseline + post-change number recorded in Discovery.
11. Free-trial state: `zombiectl doctor --json` returns `billing.free_trial.active = true` when `now_ms < FREE_TRIAL_END_MS`. Dashboard billing panel renders the "Free trial · expires 2026-07-31 (UTC)" line in the same window. After Aug 1 2026 00:00 UTC, both surfaces report `active: false` and the live rate strings.
12. `make sync-version` passes; `make check-version` passes; `VERSION` bumped; `CHANGELOG.md` entry present (lead repo) and `changelog.mdx` entry present (companion PR) following release-template.md voice.
13. Mission Control: lead-repo `grep -rln "Mission Control" .` (excluding `node_modules`, `.next`, `.zig-cache`) returns empty. Companion-PR's docs-repo grep returns empty.
14. STARTER_CREDIT scrub: lead-repo + docs-repo `grep -rln "\\\$5" -- *.md *.mdx ui/ samples/ skills/ zombiectl/ src/` shows zero hits in customer-facing copy outside the rates source-of-truth (`rates.ts`, `rates.mdx`) and the bundled release-template voice file.
15. Internal Clerk rename: `grep -rln "/v1/webhooks/clerk" .` returns empty across the repo. The new path `/v1/auth/identity-events/clerk` handles the Clerk `user.created` event end-to-end (verified via a manual signup against the deployed API after N5 is run). Customer-facing `/v1/webhooks/svix/{zombie_id}` continues to handle customer Clerk webhooks unchanged.
16. Hero CTA primary ("install in Claude Code"): click → clipboard contains `npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie` exactly; viewport scrolls to `#onboarding-flow`; toast announces clipboard success via `aria-live`. No off-page navigation.
17. Hero CTA secondary ("view a real wake (replay)"): click → router navigates to `/replay`. Page mounts; first frame renders within 200 ms; full ~75-second playback runs end-to-end; loops; restart button rewinds to ts_ms=0.
18. OpenAPI orphan tags removed: `grep -nE "^- name: (Runs|Execute|Slack|Telemetry)" public/openapi/root.yaml` returns empty. Docs.usezombie.com navigation no longer has dead-end entries for these four sections. `Tenant` tag stays (verified — referenced by `paths/tenant-provider.yaml`).

---

## Execution Plan (Ordered)

Single PR, commit history in this order so review can read it sequentially. The "Files Changed" table sections (A–M) are reference groupings; the "Sections (implementation slices)" subsection headings (§1–§9) are logical groupings; the numbered order below is the **execution** order — they do not align 1-for-1 and that is intentional, so each commit lands a coherent slice rather than a Files-Changed-row.

1. Architecture docs (K1–K5) — doc-first per CLAUDE.md Architecture Update Gate
2. Config + sample fixture (A1–A3, C1)
3. Install + List response + CLI client (A4–A8, B1's `webhook_urls` capture)
4. Install-skill body rewrite (B1–B3)
5. Internal Clerk endpoint rename (N1–N5) — separate auth plane from customer-data plane before customer-facing surfaces start citing the namespace
6. Pricing constants + Pricing component + Onboarding flow + free-trial state surface (G1, G3–G7, H1–H7)
7. Hero CTA fixes — bootstrap-copy + replay route (G11–G17)
8. READMEs + badge swap (G8, G9)
9. Mission Control → Dashboard sweep (J1–J5)
10. Dashboard chat surface (D1–D8, D8b)
11. Trigger panel multi-card (E1–E5 minus E5 ApiCard, F1–F4)
12. Test files (F2–F4, B3 eval fixtures, H2 pin tests, A8 server-side filter pin, G15 replay-page tests, G17 Hero CTA tests)
13. Release mechanics (M1–M3) — `VERSION` bump + `make sync-version`
14. Redis client audit (O1) — produces `src/queue/AUDIT.md`; read-only research; no code edits in this dimension
15. `make` verification full sweep — `make test`, `make test-integration`, `make lint`, `make memleak`, `make check-pg-drain`
16. **Ops step**: update Clerk dashboard webhook URL (N5) — before merging to main
17. Companion PR open (`~/Projects/docs/`) on `feat/m68-trigger-dx-and-free-trial`

`kishore-babysit-prs` polls after every push per the project's review cadence.

---

## Discovery

To be filled at CHORE(close):

- Final list of "Mission Control" occurrences rewritten.
- Final list of singular `trigger:` occurrences migrated.
- Exact `@assistant-ui/react` version pinned.
- Bundle-size before / after `/zombies/[id]` route.
  - **Baseline (pre-§5, 2026-05-15, Next 16.2.6 Turbopack):** first-load = **325.53 kB gzipped** (1090.55 kB raw) across 16 unique chunks (8 root main + 1 polyfill + 7 route-specific client). Budget ceiling for post-§5: **425.53 kB gzipped** (baseline + 100 kB headroom for assistant-ui runtime + primitives + adapter).
  - **Measurement method (deterministic, reproducible):** `bun run build` then run the route-bundle script — parse `.next/server/app/(dashboard)/zombies/[id]/page_client-reference-manifest.js` for all `/_next/static/chunks/*.js` and `*.css`, union with `.next/server/app/(dashboard)/zombies/[id]/page/build-manifest.json` `rootMainFiles` + `polyfillFiles`, then sum `gzip.compress(b, compresslevel=9)` of each on-disk chunk under `.next/`. The same script run pre- and post-change is the gate.
  - **Post-change:** to be filled after §5 D1–D8 land.
- Companion-PR URL.
- HMAC self-verify response times observed in CI for the install-skill eval.
- Any provider whose `gh`-equivalent terminal command needed adjustment from the design-time draft.

---

## Files Changed (final)

To be filled at CHORE(close) after `git diff --name-only origin/main` and cross-referenced against the table above. Any file not in the table requires a Discovery note explaining why it was touched.

---

## Out of Scope

- GitHub App-driven webhook registration (PAT + `gh` is the v1 mechanism; GitHub App is a separate future spec).
- OAuth-based provider onboarding in the Dashboard (premature per the trigger-ingress reframe — operator-native PAT model is the wedge posture).
- `WorkspaceIntegration` entity / per-provider install state shadowing (architecture does not shadow GitHub-side hook state; `gh api repos/.../hooks` is authoritative).
- A "Test this trigger" affordance on TriggerPanel (different mental model than steer; future spec — would render on each card, not the composer).
- New webhook providers beyond M28's registry (`github`, `linear`, `jira`, `grafana`, `slack`, `agentmail`, `clerk`). All seven ship with PROVIDER_GUIDANCE in this spec. Adding an eighth provider is a follow-up spec that lands one `provider-guidance` entry + one webhook adapter.
- **`type: api` trigger** as a declarable shape in `TRIGGER.md`. Removed from the schema for v1. Reason: the workspace-API-token UX doesn't exist today — there is no `/settings/tokens` page, no `core.workspace_tokens` table, no Bearer-auth path on `POST /v1/zombies/{id}/events`. Building all of that is a separate ~600-LOC spec with its own schema migration. The wedge (platform-ops) doesn't need `type: api`; `webhook` + `cron` cover the surface. Schema validation in A2 accepts only `webhook` and `cron` for `triggers[].type` in v1. `ApiCard.tsx` is **not** built; `CopyUrlCard` (today's State A behaviour) handles any zombie whose source the platform doesn't recognise. A future `M{N}_001_WORKSPACE_API_TOKENS` spec lands the token UI + endpoint + `type: api` schema admission together.
- A workspace API token issue/list/revoke UI. Same reason — separate concern; see above.
- Cron schedule editing in the Dashboard (today CronCard is read-only — declared in TRIGGER.md, runtime-managed by NullClaw's `cron_add` tool).
- Self-managed posture changes (M66's footprint already covers this; the strike-through banner mentions self-managed as a recommendation, no behaviour change).
- Approval-gate rendering inside the chat surface (Pending Approvals stays its own section per `[id]/page.tsx:91`; ZombieThread shows an inline system-chip *link* when approval is required, full flow remains on the Approvals panel).
- Bundle-budget enforcement infrastructure (the 220 kB ceiling is asserted in CI but the CI plumbing itself, if not present, lands in a sibling spec — record in Discovery whether the existing CI check covers it).
