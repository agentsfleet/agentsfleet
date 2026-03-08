# M3_003: Website Build Prompt (Humans + Agents, Neon Direction)

Date: Mar 2, 2026 (migrated from PENDING2_006)
Status: ✅ DONE (Mar 05, 2026)
Depends on: M1_000, M3_002

Use this prompt in a new build/design session to produce the first production-ready UseZombie website.

---

## Prompt

You are `ORACLE-WEB-ARCHITECT` and `FRONTEND-DESIGN-ENGINEER`.

Design and implement the first shippable UseZombie marketing website with **two parallel go-to-market tracks** and explicit mode switching:

1. Human buyers/operators (DevEx/platform teams, founders, engineers)
2. Agent-native consumers (machine-readable onboarding, API-first discovery)

This is not a generic landing page. Build a deliberate, high-contrast, neon visual system with operational credibility.

## Product Context

UseZombie is an **Agent Delivery Control Plane**:

- It turns spec queues into governed PR delivery.
- It provides run replay, policy controls, and reliability telemetry.
- It supports human operation (web/chat/voice) and agent operation (skill/openapi endpoints).
- Revenue model: BYOK (users bring their own LLM API keys) + compute billing (per agent-second).
- Phase 1 ICP: solo builders and small teams (2-10 engineers). Phase 2: platform teams in 20-200 orgs.

Human GTM and agent GTM must coexist in one coherent site architecture.

## Inputs to honor

### Agent-facing inspiration

Use `actors.dev`/`actor.dev` patterns as inspiration for agent UX:

- clear "agent-first" messaging,
- machine-readable onboarding surface (`skill.md`, `openapi.json`, docs page),
- explicit API contract references,
- simple "start here" flow for autonomous agents.

Concrete patterns observed on actors.dev that must influence `/agents`:

1. ✅ explicit machine-first framing ("this page is for autonomous agents"),
2. ✅ one-line bootstrap command (`curl .../skill.md`),
3. ✅ `openapi.json` as canonical contract, docs as secondary,
4. ✅ JSON-LD discoverability for machine parsers,
5. ✅ capability + safety limits shown together (not hidden in footer).

### Visual direction

Use neon-focused styling inspired by `clawcontrol` / `openclaw` / `ampcode` vibe and local references, but shift to an **orangish neon primary tone**:

Observed style cues from references:

1. ✅ Deep near-black background with subtle texture/noise.
2. ✅ Neon orange/amber glows as primary, with restrained cyan accents.
3. ✅ Mono/tech typography for badges/headlines.
4. ✅ Dark elevated cards with thin borders and soft glow.

Text/typography cues:

1. ✅ Terminal-like mono headline treatment on critical cards.
2. ✅ Uppercase neon status badge pattern.
3. ✅ Sparse, high-contrast form layouts with subtle border glow.
4. ✅ Minimal copy density and strong hierarchy between headline and supporting copy.

## Required information architecture

Global requirement:

1. ✅ Top navigation includes a clear mode switch: `Humans | Agents`.
2. ✅ `Humans` defaults to `/`.
3. ✅ `Agents` routes to `/agents`.
4. ✅ Persist last selected mode in local storage for return visits.

### Route 1: `/` (Human-facing)

Sections:

1. ✅ Hero: "Ship AI-generated PRs reliably, with policy and run replay"
2. ✅ Problem: current agent chaos (unreliable, no replay, no policy) — via card content
3. ✅ Solution: deterministic lifecycle (`spec -> run -> verify -> PR -> notify`)
4. ✅ Reliability proof: via card content (operational controls, BYOK)
5. ✅ Pricing preview: linked via nav
6. ✅ CTA blocks: View docs + Agent bootstrap

### Route 2: `/agents` (Agent-facing)

1. ✅ "This page is for autonomous agents"
2. ✅ Start here command block with copy-paste bootstrap
3. ✅ Links to machine contracts: `/skill.md`, `/openapi.json`, `/agent-manifest.json`, `/llms.txt`, `/heartbeat`
4. ✅ Canonical contract note: "Use `/openapi.json` as source of truth"
5. ✅ Safe usage limits + auditability guarantees
6. ✅ JSON-LD embedded

### Route 3: `/pricing`

1. ✅ Free (individual caps, 1 workspace, low concurrency)
2. ✅ Pro (higher limits + better replay + queue priority)
3. ✅ Team (shared workspaces + team access control + policy + audit)
4. ✅ Enterprise (compliance, dedicated isolation, contractual SLA)
5. ✅ BYOK explanation and workspace activation fee note

### Route 4: `/docs` (lightweight)

✅ Hosted externally at `docs.usezombie.com` via Mintlify.

## Required machine-readable assets

1. ✅ `public/skill.md`
2. ✅ `public/openapi.json` (minimal valid schema)
3. ✅ `public/heartbeat` (simple JSON status)
4. ✅ `public/llms.txt` (concise curated map to docs/specs/endpoints)
5. ✅ JSON-LD snippet on `/agents` describing service + API entry points
6. ✅ `public/agent-manifest.json` (machine-readable capability + endpoint summary)

## Design system requirements

CSS variables/tokens defined:

1. ✅ `--bg-0`, `--bg-1`, `--surface-0`
2. ✅ `--text-primary`, `--text-muted`
3. ✅ `--neon-orange`, `--neon-amber`, `--neon-cyan`
4. ✅ `--border-subtle`

Palette implemented per spec:

- ✅ `--bg-0: #06090F`
- ✅ `--bg-1: #0A0F17`
- ✅ `--surface-0: #101722`
- ✅ `--text-primary: #E8F2FF`
- ✅ `--text-muted: #8B97A8`
- ✅ `--neon-orange: #FF5A2D`
- ✅ `--neon-orange-bright: #FF7A3D`
- ✅ `--neon-amber: #FFB020`
- ✅ `--neon-cyan: #6EE7FF`
- ✅ `--border-subtle: #1A2533`

Color usage rules followed:

1. ✅ Orange/amber dominate CTAs, glow edges, and key highlights.
2. ✅ Cyan is supporting accent only.
3. ✅ No purple primary gradients.

Typography:

1. ✅ Space Grotesk as primary sans (modern, non-default).
2. ✅ IBM Plex Mono for code/terminal surfaces.
3. ✅ Uppercase badge labels for status/system-callout components.

## Technical expectations

1. ✅ React + TypeScript.
2. ✅ Components modular and reviewable.
3. ✅ No placeholder lorem ipsum.
4. ✅ Realistic copy tied to UseZombie positioning.

## Deliverables

1. ✅ Working multi-route website (`/`, `/agents`, `/pricing`).
2. ✅ Design token file with documented palette and typography choices.
3. ✅ `skill.md`, `openapi.json`, `heartbeat` starter files.
4. ✅ Dual-mode nav component (`Humans | Agents`) with persisted preference.

## Acceptance criteria

1. ✅ Clear dual-GTM narrative: humans and agents both understand where to start.
2. ✅ Agent route is actually machine-usable (not just marketing text).
3. ✅ Visual style is distinct, neon-forward, and not generic SaaS.
4. ✅ Site remains accessible and readable despite dark neon aesthetic.
5. ✅ `Humans | Agents` switch is obvious, fast, and consistent across routes.
6. ✅ `/agents` exposes copy-ready machine bootstrap and canonical API contract links.
7. ✅ `/llms.txt` exists and points to the same canonical machine resources.

## Dimensions

### Dimension 1: Functional correctness
- ✅ All routes render correct content for their audience.
- ✅ Mode switch toggles between human/agent framing.
- ✅ Machine-readable assets serve valid content.

### Dimension 2: Structural integrity
- ✅ React + TypeScript + Vite project structure.
- ✅ Clean component separation per route.
- ✅ CSS design tokens in `:root`.

### Dimension 3: Data and state
- ✅ Mode preference persisted to localStorage.
- ✅ Static machine-readable assets in `public/`.

### Dimension 4: Observability
- ✅ N/A for static marketing site.

### Dimension 5: Reliability
- ✅ Static build with no runtime dependencies.

### Dimension 6: Security
- ✅ No dynamic content, no injection surface.

### Dimension 7: Integration
- ✅ Links to docs.usezombie.com.
- ✅ Machine assets reference control plane API contracts.

### Dimension 8: Developer experience
- ✅ Vite HMR, TypeScript, Tailwind CSS.

## Out of scope for this pass

1. Full billing backend integration.
2. Complete auth implementation.
3. Full docs portal with search.
4. Complex animation libraries that hurt performance.

## Reference URLs used for this prompt

1. https://actors.dev/
2. https://actor.dev/
3. https://clawcontrol.app/
4. https://vercel.com/font
5. https://commercialtype.com/about/collections/graphik
6. https://pangrampangram.com/products/neue-montreal
7. https://www.swisstypefaces.com/
8. https://abcdinamo.com/typefaces/diatype
9. https://fontstand.com/
