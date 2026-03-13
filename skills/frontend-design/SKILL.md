---
name: frontend-design
description: Design and implement distinctive, production-grade web UI with accessibility and responsive behavior built in.
---

# Frontend Design

Create distinctive, production-grade web UI with strong accessibility and clear stack defaults.

## What This Skill Helps With

- Turning product requirements into shippable UI code
- Building pages/components that are visually intentional (not boilerplate)
- Enforcing accessibility and responsive behavior at implementation time
- Providing design rationale + complete code, not mockups

## Invocation Contract

- Primary invoker: `Oracle` (the active coding agent in this repo)
- Triggered when user asks for:
  - New website UI, landing pages, dashboards, or design systems
  - A frontend redesign with implementation code
  - Accessibility-first frontend implementation
- May also be called inside scaffold work when a scaffold includes a web UI surface

## Important Exception

If the target repo already uses Angular + Material UI, do not enforce this skill's visual direction as a restyle mandate.

- Keep existing Angular/Material patterns
- Prioritize accessibility, consistency, and maintainability over aesthetic replacement
- Use this skill only to improve implementation quality within that design system

## Default Website Stack

- React 19+ + TypeScript
- Vite 7+ with `@tailwindcss/vite` (no PostCSS, no `tailwind.config.ts`)
- TailwindCSS v4 — CSS-first (`@import "tailwindcss"` in styles.css)
- CSS custom properties for brand tokens (no hardcoded colors)
- React Hook Form + Zod for forms

## Design System Defaults

### Typography

Use the **Geist** font pair. Load via Google Fonts CDN — place `@import url(...)` **before** `@import "tailwindcss"` (CSS spec: `@import` rules must precede all other rules).

```css
@import url("https://fonts.googleapis.com/css2?family=Geist:wght@300;400;500;600;700&family=Geist+Mono:wght@400;500&display=swap");
@import "tailwindcss";

@theme {
  --font-sans: "Geist", system-ui, sans-serif;
  --font-mono: "Geist Mono", "Fira Code", monospace;
}
```

Alternative mono CDN (if Google Fonts unavailable):
```css
@font-face {
  font-family: "Geist Mono";
  src: url("https://cdn.jsdelivr.net/npm/geist@1/dist/fonts/geist-mono/GeistMono-Regular.woff2") format("woff2");
}
```

### Color Tokens

Define brand tokens as CSS custom properties on `:root`. Never hardcode hex values in components.

```css
:root {
  /* Brand */
  --neon-orange: #FF6B35;
  --neon-orange-dim: rgba(255, 107, 53, 0.15);
  --neon-orange-glow: rgba(255, 107, 53, 0.4);

  /* Terminal / Agent surface */
  --terminal-green: #39FF85;
  --terminal-green-dim: rgba(57, 255, 133, 0.1);

  /* Neutrals */
  --bg-base: #0A0A0A;
  --bg-surface: #111111;
  --bg-card: #161616;
  --border-subtle: rgba(255, 255, 255, 0.08);
  --border-default: rgba(255, 255, 255, 0.12);
  --text-primary: #F5F5F5;
  --text-secondary: rgba(245, 245, 245, 0.6);
  --text-muted: rgba(245, 245, 245, 0.35);
}
```

### Background: Dot-Grid

Apply a dot-grid texture to the page via a `body::before` pseudo-element. No JS libraries needed.

```css
body::before {
  content: "";
  position: fixed;
  inset: 0;
  background-image: radial-gradient(
    circle,
    rgba(255, 255, 255, 0.06) 1px,
    transparent 1px
  );
  background-size: 24px 24px;
  pointer-events: none;
  z-index: 0;
}
```

### Hero Glow

Ambient glow behind hero content — creates depth without a heavy image.

```css
.hero-glow {
  position: absolute;
  top: -20%;
  left: 50%;
  transform: translateX(-50%);
  width: 800px;
  height: 600px;
  background: radial-gradient(
    ellipse at center,
    rgba(255, 107, 53, 0.12) 0%,
    transparent 70%
  );
  pointer-events: none;
}
```

### Card Hover

Cards lift with a subtle border glow on hover. No JS needed.

```css
.card {
  background: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: 12px;
  transition: border-color 0.2s, box-shadow 0.2s;
}
.card:hover {
  border-color: rgba(255, 107, 53, 0.3);
  box-shadow: 0 0 20px rgba(255, 107, 53, 0.08);
}
```

### Agent / Terminal Surface

For agent-facing pages or terminal-aesthetic sections:

```css
.agent-surface {
  background: var(--bg-surface);
  border: 1px solid var(--terminal-green-dim);
  font-family: var(--font-mono);
  color: var(--terminal-green);
}

/* Optional scanline overlay */
.agent-surface::after {
  content: "";
  position: absolute;
  inset: 0;
  background: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 2px,
    rgba(0, 0, 0, 0.03) 2px,
    rgba(0, 0, 0, 0.03) 4px
  );
  pointer-events: none;
}
```

### Design Direction Summary

| Decision | Choice | Why |
|---|---|---|
| Font sans | Geist | Clean, modern, developer-native |
| Font mono | Geist Mono | Consistent with sans; strong code aesthetic |
| Accent | `#FF6B35` (neon orange) | Warm, energetic, differentiates from blue-heavy SaaS |
| Agent accent | `#39FF85` (terminal green) | Classic terminal signal color |
| Background | `#0A0A0A` + dot-grid | Dark-native, depth without imagery |
| Glow style | Radial orange ambient | Soft energy, not garish |
| Border style | `rgba(white, 0.08–0.12)` | Subtle depth without harsh lines |

**Anti-patterns for dark dev-tool sites:**
- White backgrounds or light-mode default
- Blue as the only accent (overused in SaaS)
- Flat cards with no depth signal
- System fonts on a design-forward landing page
- Full-bleed hero images (prefer CSS glow + grid texture)

## Standard Make Targets (web)

All web repos must expose these targets in `Makefile`:

```makefile
make dev          # bun run dev (Vite dev server, port 5173)
make lint         # bun run lint && bun run typecheck
make test         # bun run test (vitest unit)
make build        # bun run build (tsc + vite build)
make qa           # bun run test:e2e (Playwright full suite, headless)
make qa-smoke     # bun run test:e2e:smoke (smoke only, CI gate)
make _clean       # rm -rf dist coverage node_modules
```

`make up` / `make down` only needed when Docker services are part of the stack.

## Accessibility Contract (Required)

- Semantic HTML landmarks and headings
- WAI-ARIA attributes only where native semantics are insufficient
- WCAG 2.2 AA baseline
- Keyboard-only navigation support
- Minimum touch target: `44x44px`
- Automated web accessibility checks with `@axe-core/playwright`

Example check:

```bash
bun add -d @axe-core/playwright
make qa   # or: bunx playwright test --reporter=line
```

## Output Contract

When this skill is used, return:

1. Design intent (concept + visual direction)
2. Complete implementation code (no placeholders)
3. State coverage (default/hover/loading/error/disabled/empty)
4. Usage example
5. Accessibility notes and verification commands

## Anti-Patterns To Avoid

- Generic white-page + blue-button templates
- Inconsistent typography systems
- Hardcoded colors without tokens
- Missing loading/error/empty states
- Non-semantic markup
- Keyboard-inaccessible interactions
