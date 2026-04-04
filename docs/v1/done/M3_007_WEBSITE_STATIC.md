# M3_007: Static Website — usezombie.com + usezombie.sh

Date: Mar 4, 2026
Status: ✅ DONE (Mar 05, 2026)
Priority: P1 — v1 requirement (must be live before launch)
Depends on: None (can be built independently of backend work)

---

## Problem

UseZombie has no web presence. Two domains need to be active for v1 launch:

- `usezombie.com` — human buyers, product landing page.
- `usezombie.sh` — agent-first discovery surface, machine-readable onboarding.

The Mission Control UI (`app.usezombie.com` or `app.usezombie.ai`) is v3 scope. v1 is CLI-first.

## Solution

Static website deployed to Vercel (or Cloudflare Pages). No backend. No authentication. No dynamic content. Built with React + TypeScript per M3_003 design direction.

### Domain Mapping

| Domain | Purpose | Content |
|---|---|---|
| `usezombie.com` | Primary marketing | `/` landing, `/pricing`, links to docs |
| `usezombie.sh` | Agent discovery | `/agents`, machine-readable assets |
| `docs.usezombie.com` | Technical docs | Mintlify-hosted |
| `api.usezombie.com` | API endpoint | zombied API (not website) |
| `app.usezombie.com` | Mission Control UI | v3 — not in v1 scope |

### Routes (v1 Static)

| Route | Domain | Purpose |
|---|---|---|
| `/` | usezombie.com | Hero, problem, solution, pricing preview, CTAs |
| `/pricing` | usezombie.com | Free/Pro/Team/Enterprise comparison |
| `/agents` | usezombie.sh | Machine-first bootstrap, copy-paste commands |
| `/skill.md` | usezombie.sh | LLM-readable onboarding instructions |
| `/openapi.json` | usezombie.sh | OpenAPI 3.1 spec |
| `/agent-manifest.json` | usezombie.sh | JSON-LD agent discovery |
| `/llms.txt` | usezombie.sh | LLM-friendly site summary |
| `/heartbeat` | usezombie.sh | Static health check |

### Machine-Readable Assets

Copied from `public/` directory in the monorepo. Served as static files:

- `public/openapi.json` → `usezombie.sh/openapi.json`
- `public/agent-manifest.json` → `usezombie.sh/agent-manifest.json`
- `public/skill.md` → `usezombie.sh/skill.md`
- `public/llms.txt` → `usezombie.sh/llms.txt`
- `public/heartbeat` → `usezombie.sh/heartbeat`

### Visual Direction

Per M3_003:
- Dark near-black background, neon orange primary.
- Mono/tech typography (Geist Sans + Geist Mono — free).
- Code blocks and terminal output as visual elements.
- No "powered by AI" or marketing fluff.

### Tech Stack

- React + TypeScript + Vite (static build)
- Tailwind CSS for styling
- Deployed to Vercel (free tier sufficient for static)
- Separate Vercel project from future Mission Control UI

### Repository

Option A: `website/` directory in usezombie monorepo.
Option B: Separate repo `usezombie/website`.

**Recommended:** Option A (monorepo) — keeps machine-readable assets in sync with `public/` directory.

### DNS (Cloudflare)

```
usezombie.com    → Vercel (website)
usezombie.sh     → Vercel (website, /agents route)
docs.usezombie.com → Mintlify
api.usezombie.com  → Railway/Fly/Render (zombied API)
```

## CTAs

| CTA | Target | Route |
|---|---|---|
| "Get started" | `npx zombiectl login` (copy-paste) | `/` |
| "View docs" | `docs.usezombie.com` | `/` |
| "Book team pilot" | Calendly or email | `/pricing` |
| "Bootstrap" | `npx zombiectl login` (terminal block) | `/agents` |

## Acceptance Criteria

1. `usezombie.com` loads with landing page, pricing.
2. `usezombie.sh` loads with `/agents` route and machine-readable assets.
3. `usezombie.sh/openapi.json` returns valid OpenAPI 3.1 spec.
4. `usezombie.sh/agent-manifest.json` returns valid JSON-LD.
5. `usezombie.sh/llms.txt` returns LLM-friendly text.
6. Mobile-responsive, WCAG 2.2 AA accessible.
7. No JavaScript required for machine-readable assets (static files).
8. DNS configured via Cloudflare.
9. HTTPS enforced on all domains.

## Out of Scope

- Mission Control UI (`app.usezombie.com`) — v2.
- User authentication on website — v2.
- Blog or content marketing — post-v1.
- Interactive demos with live backend — post-v1.
