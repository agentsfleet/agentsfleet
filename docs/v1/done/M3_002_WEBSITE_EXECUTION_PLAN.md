# M3_002: Website Execution Plan (Human + Agent Boundaries)

Date: Feb 28, 2026
Status: ✅ DONE (Mar 05, 2026)

---

## Goal
Define M1 website execution boundaries so human buyers and autonomous agents each receive a clear, purpose-built onboarding path while sharing one consistent control-plane contract and machine-readable discovery surface.

## Explicit assumptions
1. Website IA is route-first: `/` for humans, `/agents` for autonomous agents.
2. Orange-neon visual direction remains primary (`--neon-orange` family).
3. Machine-readable assets are first-class deliverables, not optional docs.
4. The website presents existing M1 control-plane contracts; it does not invent new runtime behavior.
5. `/agents` copy is concise and machine bootstrap oriented.

## In-scope
1. Route plan and content contract for `/`, `/agents`, `/pricing`, `/docs`.
2. Required machine-readable assets: `openapi.json`, `agent-manifest.json`, `skill.md`, `llms.txt`, JSON-LD on `/agents`.
3. Human/agent mode boundaries and CTA flows.
4. Accessibility and responsive acceptance requirements tied to this IA.

## Out-of-scope
1. Full marketing CMS and blog architecture.
2. Advanced onboarding auth flows and billing checkout.
3. Multi-language localization.
4. Interactive product demo with live backend dependencies.

## Implementation dependency
Website implementation starts **after** the control plane MVP ships its first successful PR. Machine-readable assets (`openapi.json`, `skill.md`, `llms.txt`) require a working API to reference. Build the API first, then the website that describes it.

## Interfaces and contracts
### 1) Route contract
1. `/` (human-facing): problem framing, reliability value prop, GTM narrative, CTA to `/pricing` and `/docs`. ✅
2. `/agents` (agent-facing): explicit machine-first framing, quick bootstrap steps, canonical API links. ✅
3. `/pricing`: Free/Pro/Team/Enterprise packaging with conversion triggers. ✅
4. `/docs`: concise technical orientation and links to full specs. ✅ (Mintlify-hosted at docs.usezombie.com)

### 2) Machine-readable asset contract
1. `public/openapi.json`: canonical API schema for control plane operations. ✅
2. `public/agent-manifest.json`: machine-readable capability + endpoint summary. ✅
3. `public/skill.md`: minimal agent bootstrap instructions. ✅
4. `public/llms.txt`: curated index to machine-relevant docs/endpoints. ✅
5. JSON-LD on `/agents`: service + API entry points aligned to OpenAPI. ✅

Alignment rules:
1. Endpoint paths and operation IDs must match `M1_002_API_AND_EVENTS_CONTRACTS.md` contract.
2. `/agents` and `/llms.txt` must reference same canonical URLs.
3. Version fields must stay synchronized across machine-readable assets.

### 3) Human/agent mode boundary contract
1. Human flow (`/`): benefits, trust proof, pricing path. ✅
2. Agent flow (`/agents`): actionability, deterministic contracts, no marketing fluff. ✅
3. Shared truth: both modes resolve to same API/policy semantics. ✅
4. No route may contradict control-plane state machine or policy classes. ✅

### 4) Execution sequence contract
1. Build route skeleton and shared design tokens. ✅
2. Implement `/agents` machine-first content and JSON-LD. ✅
3. Add machine-readable files under `public/`. ✅
4. Complete `/pricing` and `/docs` with contract links. ✅
5. Run accessibility/responsive and link integrity checks. ✅

## Acceptance criteria
1. ✅ Dual GTM narrative is clear: humans start at `/`, agents start at `/agents`.
2. ✅ `/agents` includes copy-ready bootstrap + canonical links to OpenAPI and machine assets.
3. ✅ `llms.txt` exists and points to the same machine resources referenced by `/agents`.
4. ✅ Route-level CTAs do not cross-wire user intent (human vs agent).
5. ✅ Accessibility checks pass for contrast, keyboard nav, and semantic headings.
6. ✅ Mobile and desktop layouts both preserve IA clarity and CTA visibility.

## Risks and mitigations
1. Risk: marketing copy drifts into `/agents` and weakens machine usability.
Mitigation: enforce route content checklist with machine-first constraints.
2. Risk: machine-readable asset drift after API updates.
Mitigation: CI link and schema consistency checks against OpenAPI.
3. Risk: visual style overwhelms readability.
Mitigation: hard contrast thresholds and constrained neon usage.
4. Risk: human/agent boundary becomes ambiguous.
Mitigation: explicit route labels, hero copy, and separated CTAs.

## Dimensions

### Dimension 1: Functional correctness
- ✅ Routes `/`, `/agents`, `/pricing` implemented with correct content boundaries.
- ✅ Mode switch persists to localStorage.
- ✅ Machine-readable assets served from `public/`.

### Dimension 2: Structural integrity
- ✅ Route skeleton with shared design tokens.
- ✅ Component separation (Home, Agents, Pricing).
- ✅ JSON-LD embedded on `/agents`.

### Dimension 3: Data and state
- ✅ Mode preference persisted via localStorage.
- ✅ Static assets (openapi.json, skill.md, llms.txt, heartbeat, agent-manifest.json) in `public/`.

### Dimension 4: Observability
- ✅ N/A for static site (no runtime telemetry needed).

### Dimension 5: Reliability
- ✅ Static build — no runtime failures possible.
- ✅ Machine-readable assets are plain files, no server dependency.

### Dimension 6: Security
- ✅ No auth, no dynamic content, no injection surface.
- ✅ HTTPS enforced via deployment platform.

### Dimension 7: Integration
- ✅ Links to docs.usezombie.com, api.usezombie.com.
- ✅ OpenAPI and agent-manifest reference control plane contracts.

### Dimension 8: Developer experience
- ✅ Vite dev server with HMR.
- ✅ TypeScript strict mode.
- ✅ Tailwind CSS for styling.

## Test/verification commands
```bash
# Required section presence
rg -n "^## (Goal|Explicit assumptions|In-scope|Out-of-scope|Interfaces and contracts|Acceptance criteria|Risks and mitigations|Test/verification commands)$" docs/done/v1/M3_002_WEBSITE_EXECUTION_PLAN.md

# IA and machine-readable assets are defined
rg -n "/agents|/pricing|/docs|openapi.json|agent-manifest.json|skill.md|llms.txt|JSON-LD" docs/done/v1/M3_002_WEBSITE_EXECUTION_PLAN.md

# Boundary and styling constraints captured
rg -n "human|agent|neon-orange|accessibility|responsive|CTA" docs/done/v1/M3_002_WEBSITE_EXECUTION_PLAN.md
```
