# Design System — Operational Restraint

**Version:** 0.1 · 2026-05-08
**Source of truth.** All visual, typographic, and motion decisions in `ui/packages/website`, `ui/packages/app`, `ui/packages/design-system`, `docs.usezombie.com`, and `zombiectl` output read from this document. Do not deviate without explicit user approval and a corresponding update here.

---

## Memorable thing

**"It wakes."** A long-lived daemon that wakes on events, runs against a durable replayable log, and posts evidenced answers — never chats. Every visual decision serves this posture.

---

## Product context

- **What:** Always-on operational runtime. Zombies are long-lived daemons that own one operational outcome end to end.
- **Who for:** Engineers running production infrastructure who want events → evidence → diagnosis without wiring a chatbot.
- **Category:** Developer infrastructure / observability adjacent.
- **Surfaces:**
  - `ui/packages/website` — marketing site (`usezombie.com`)
  - `ui/packages/app` — authenticated product UI (`app.usezombie.com`)
  - `ui/packages/design-system` — shared React component library
  - `docs.usezombie.com` — long-form technical documentation
  - `zombiectl` — CLI output (rendered in 256-color terminals)

---

## Aesthetic direction

**"Operational Restraint"** — serious infrastructure brand language with one signature of liveness nobody else owns.

- **Reference vibes:** Anthropic console × Datadog × a single bioluminescent pulse.
- **Anti-vibes:** Vercel/Linear aurora gradients, purple-to-blue meshes, "magical" hero animations, friendly mascots, chat bubbles, decorative blobs, gradient CTA buttons, bubble-radius everything.
- **Decoration level:** minimal. The mono typography + the pulse do all the work. A subtle dot-grid background is permitted on marketing hero only (8% opacity).
- **Mood:** evidenced, machine-precise, slightly haunted, never decorative. The product feels alive but never performs.
- **Differentiation strategy:** restraint as the differentiator. Every competitor uses aurora gradients. By having none, the single pulse color owns all attention.

---

## Typography

### Font stack

| Role | Font | Weights | License | Source |
|---|---|---|---|---|
| Display, UI chrome (buttons, labels, badges, nav, headers) | **Commit Mono** | 400, 500, 600, 700 | OFL (free) | https://commitmono.com |
| Body, paragraphs, long-form copy | **Instrument Sans** | 400, 500, 600 | OFL (free) | https://fonts.google.com/specimen/Instrument+Sans |
| Code, logs, data tables | **Commit Mono** (same family — keeps the system tight) | 400, 500 | OFL (free) | https://commitmono.com |

**Optional commercial upgrade:** swap Commit Mono → **Berkeley Mono** (~$300 commercial team license, https://berkeleygraphics.com). Spec is font-agnostic; only the file changes. Recommended only if the user explicitly asks for the peak signal — Commit Mono is intentionally chosen so the entire stack ships free.

**No-fly list (never use, even if requested without explicit override):**
- **Geist / Geist Mono** — currently in `ui/packages/website` and `ui/packages/app`. Replace during implementation. Overused; the new Inter.
- Inter, Inter Tight, Roboto, Arial, Helvetica, Open Sans, Lato, Montserrat, Poppins
- Space Grotesk (the AI-design convergence trap — every AI tool defaults to it)
- system-ui / -apple-system as the primary display or body face (the "I gave up on typography" signal)

### Type scale

| Token | Family | Size / Line / Tracking | Weight | Use |
|---|---|---|---|---|
| `display-xl` | Commit Mono | 64 / 1.0 / -0.025em | 500 | Marketing hero only |
| `display-lg` | Commit Mono | 40 / 1.1 / -0.02em | 500 | Section heads on marketing & docs |
| `display-md` | Commit Mono | 28 / 1.15 / -0.015em | 500 | Stat values, inline metric callouts |
| `heading` | Commit Mono | 18 / 1.3 / 0 | 500 | App page titles, card heads |
| `eyebrow` | Commit Mono | 12 / 1.3 / 0.08em uppercase | 500 | Section labels, status eyebrow on hero |
| `body-lg` | Instrument Sans | 18 / 1.5 / 0 | 400 | Marketing lede, long-form intros |
| `body` | Instrument Sans | 15 / 1.55 / 0 | 400 | Default body text |
| `body-sm` | Instrument Sans | 13 / 1.5 / 0 | 400 | Secondary body, helper text |
| `label` | Commit Mono | 11 / 1.3 / 0.08em uppercase | 500 | Form labels, stat labels |
| `mono` | Commit Mono | 13 / 1.55 / 0 + tabular-nums | 400 | Code, logs, data, badges |

Apply `font-feature-settings: "tnum"` (or Tailwind `tabular-nums`) on every numeric column, stat value, dashboard row, and CLI table.

---

## Color

Dark is the **primary** brand surface. All hero shots, marketing screenshots, docs landing pages, and the canonical app screenshot ship dark. Light mode exists and is fully supported, but is never the brand's first impression.

### Dark mode tokens

| Token | Hex | Use |
|---|---|---|
| `--bg` | `#0A0D0E` | Page background. Near-black, cool undertone. Never use pure `#000`. |
| `--surface-1` | `#11161A` | Default elevated surface (cards, sidebars). |
| `--surface-2` | `#181E22` | Inputs, mockup chrome, elevated cards. |
| `--surface-3` | `#1F262C` | Hover state, more-elevated layer. |
| `--border` | `#23292E` | Default borders. |
| `--border-strong` | `#2E373E` | Active/focused borders, button outlines. |
| `--text` | `#E6EAEC` | Default text. Off-white, never pure `#FFF`. |
| `--text-muted` | `#8B9398` | Secondary text, captions. |
| `--text-subtle` | `#5C6469` | Tertiary text, timestamps, dim CLI output. |

### The pulse — used only on live signals

| Token | Hex | Rule |
|---|---|---|
| `--pulse` | `#5EEAD4` | **Bioluminescent cyan-mint.** The signature accent. Used **only** on live/awake/wake signals: pulse rings on running zombies, `LIVE` badges, the brand-mark dot, primary CTA buttons, link color, focus rings. Treat as currency — every additional use dilutes. |
| `--pulse-dim` | `#2DD4BF` | Pressed state for primary buttons; pulse desaturated. |
| `--pulse-glow` | `rgba(94, 234, 212, 0.35)` | The expanding ring color in the wake-pulse keyframe. |

**Forbidden uses of `--pulse`:** decorative borders, large background fills, gradient stops, hover states on non-live elements, illustrations.

### Status (use sparingly)

| Token | Hex | Use |
|---|---|---|
| `--success` | `#34D399` | Success log lines, OK states, deltas trending good. |
| `--warn` | `#F59E0B` | Degraded zombies, warning logs. |
| `--error` | `#F87171` | Failed zombies, error logs. |
| `--info` | `#60A5FA` | Debug logs, neutral informational. |
| `--evidence` | `#FBBF24` | Warm amber. Reserved for evidence-quoted content (line-numbered logs, citation marks, `EVIDENCE` log labels). |

### Light mode (secondary)

| Token | Hex | Notes |
|---|---|---|
| `--bg` | `#F8F6F1` | Warm parchment, never pure white. Reinforces "evidenced document" feel. |
| `--surface-1` | `#F1EEE6` | |
| `--surface-2` | `#E9E5DA` | |
| `--surface-3` | `#DFDACB` | |
| `--border` | `#D4CDB9` | |
| `--border-strong` | `#B7AE96` | |
| `--text` | `#1A1D1E` | |
| `--text-muted` | `#5A625F` | |
| `--text-subtle` | `#8A918A` | |
| `--pulse` | `#14B8A6` | Pulse desaturates 15% in light mode. |
| `--pulse-dim` | `#0D9488` | |
| `--pulse-glow` | `rgba(20, 184, 166, 0.30)` | |

### Forbidden color treatments

- Aurora gradients (purple-to-blue mesh) — anywhere
- Three-or-more-stop gradients on any surface or button
- `--pulse` used as a large fill (it's currency, not paint)
- Pure `#000` background or pure `#FFF` background
- Status colors used decoratively (only for actual status)
- Multiple accent colors competing for attention

---

## Spacing

- **Base unit:** **4px** (not 8px — engineers want information density, not cathedral whitespace)
- **Density:** comfortable-dense
- **Scale:** `2 / 4 / 8 / 12 / 16 / 24 / 32 / 48 / 64 / 96` (px)
- **Rhythm:** primary section vertical padding is 96px on marketing, 48px on app, 32px on dense data views.

---

## Layout

- **Marketing site:** editorial within a 12-col grid. Asymmetry permitted in hero only; strict grid everywhere else.
- **App / dashboard:** strict 12-col grid. Borders > shadows. Tabular-nums on every numeric column. Comfortable-dense rows; no `padding: 24px` rows when 12px holds the same information.
- **Docs:** single-column, ~68ch measure (`max-width: 720px` at default body size). Commit Mono headers, Instrument Sans body.
- **CLI / zombiectl:** 256-color palette mirroring web tokens. Pulse cyan for live state, amber for `EVIDENCE` lines, status colors restrained, no decorative ASCII art (no boxes-around-titles, no banners, no ASCII zombies).
- **Max content width (marketing & docs):** 1280px.
- **Border radius:** small and hierarchical. `--r-sm: 2px`, `--r-md: 4px`, `--r-lg: 6px`. **No `border-radius: 9999px` on buttons. Ever.** Only on circular dots, avatars, status rings.
- **Borders preferred over shadows.** A `1px solid var(--border)` is the default elevation cue. Drop shadows are only for floating elements (popovers, modals).

---

## Motion

The system has **one** signature animation. Everything else is functional or absent.

### Wake pulse (signature)

```css
@keyframes pulse {
  0%   { box-shadow: 0 0 0 0 var(--pulse-glow); }
  50%  { box-shadow: 0 0 0 10px transparent; }
  100% { box-shadow: 0 0 0 0 transparent; }
}
.live { animation: pulse 2.4s ease-in-out infinite; }
```

**Rules:**
- Fires only on actually-live entities (running zombies, active streams, `LIVE` badges, the brand-mark dot in the header, the cursor on the hero).
- The instant a zombie is parked, the animation stops.
- Failed/degraded zombies get a static ring in their status color; no pulse.
- Maximum on-screen: ~5 simultaneous pulses. More than that is visual noise; consolidate to a count.

### Functional motion

- **Hovers:** `transition: 50ms ease-out`. Snap. No bounce, no spring, no `cubic-bezier` overshoot.
- **Focus rings:** instant, no animated draw-on. `box-shadow: 0 0 0 3px var(--pulse-glow)`.
- **Page transitions:** instant. No fade, no slide. Operational software does not perform.
- **Log streams:** new lines fade in over 80ms (`opacity 0 → 1`). No slide-up, no stagger.
- **Loading states:** prefer skeleton bars (1-pixel-thick borders) over spinners. If a spinner is required, use a 2px monochrome arc, not a circular pulse.

### `prefers-reduced-motion: reduce`

- Pulse animations become a static ring at `0.2` opacity (`box-shadow: 0 0 0 4px var(--pulse-glow)`).
- Log-stream fade disabled.
- Hover transitions retained at 50ms (functional, not decorative).

### Forbidden motion

- Bouncy easings (`elastic`, `bounce`, anything with overshoot)
- Page-transition fades or slides
- Scroll-driven animations on marketing (other than the static dot-grid)
- Animated gradients
- Cursor-following effects, parallax, mouse-tracking glows
- Spring physics on UI chrome

---

## Component principles

- **Buttons:** mono font, 13px, padding `12px 16px`, border-radius `--r-md`. Three variants: `primary` (pulse fill, near-black text), `default` (surface-2 fill, border-strong outline), `ghost` (transparent, muted text). No gradient buttons. No icon-only buttons larger than 36px square.
- **Badges:** mono font, 11px, padding `4px 8px`, border-radius `--r-sm`. Status badges (`LIVE`, `degraded`, `failed`) get colored fills; informational badges get muted outlines.
- **Form fields:** surface-2 background, border on default, pulse-cyan focus ring with `--pulse-glow` shadow. Mono font for input values (they're operational data, not prose).
- **Cards:** surface-1 background, 1px border, `--r-md` radius. Padding 24px default, 16px in dense data views.
- **Tables / lists:** prefer flat rows with 1px bottom borders over zebra-striping. Tabular-nums everywhere. Right-align numbers, left-align text.
- **Sidebars:** surface-2 background. Mono nav items, 12px. Active item gets surface-3 fill, not a colored bar.

---

## CLI / zombiectl rendering

- **Palette mapping** (256-color terminal):
  - `--pulse` → `#5EEAD4` (closest 256: 79 / `cyan2`)
  - `--evidence` → `#FBBF24` (closest 256: 220 / `gold1`)
  - `--success` → `#34D399` (closest 256: 78)
  - `--warn` → `#F59E0B` (closest 256: 214)
  - `--error` → `#F87171` (closest 256: 210)
  - `--text-muted` → `#8B9398` (closest 256: 102 / `grey53`)
  - `--text-subtle` → `#5C6469` (closest 256: 240)
- **Status glyphs:**
  - Live: `●` in `--pulse`
  - Parked: `○` in `--text-subtle`
  - Degraded: `●` in `--warn`
  - Failed: `✕` in `--error`
- **`EVIDENCE` lines:** `EVIDENCE` label in `--evidence`, source ref in `--text`, quoted content in `--text-muted`.
- **No decorative ASCII art** — no zombie face, no boxes around titles, no banners, no rocket emoji. The CLI is operational output.

---

## Implementation roadmap (separate effort from this doc)

This document is the spec. Implementation is a separate milestone. Suggested workstream split:

1. **W1 — `ui/packages/design-system`:** rewrite `tokens.css`, `theme.css`. Swap `@fontsource-variable/geist` for Commit Mono (self-host) + Instrument Sans (Google Fonts or self-host). Update every component (Button, Badge, Card, Input, etc.) to read new tokens. Add `<WakePulse />` primitive for the signature animation.
2. **W2 — `ui/packages/website`:** apply new tokens, replace any Geist references, rebuild marketing hero with the new typography scale, add the dot-grid hero background.
3. **W3 — `ui/packages/app`:** apply new tokens, audit every page against the dashboard mockup, ensure `<WakePulse />` only fires on actually-live zombies (data-driven, not decorative).
4. **W4 — `docs.usezombie.com`:** apply new typography stack, single-column layout, ~68ch measure.
5. **W5 — `zombiectl`:** add 256-color terminal mode (detect via `tput colors`), implement status glyphs, audit every output line for the new palette mapping.
6. **W6 — Wire-up:** add a `docs/DESIGN_SYSTEM.md` row to the EXECUTE doc-reads table in `AGENTS.md` (triggers: `*.tsx`, `*.css`, files under `ui/packages/**`, `zombiectl/src/**` when touching output formatting). Triggers the Invariance Suite Gate — handle as its own commit.

Each workstream is its own spec. Use `kishore-spec-new` to create them once you're ready to start implementation.

---

## Decisions log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-08 | Initial design system created — Operational Restraint direction | Created via `/design-consultation`. User picked dark-primary, operational mono display, restrained zombie metaphor (one pulse signal), single bioluminescent accent. Memorable thing locked as "It wakes." |
| 2026-05-08 | Drop Geist (currently in `ui/packages/website`, `ui/packages/app`) | Overused; the new Inter. Replaced with Commit Mono + Instrument Sans. |
| 2026-05-08 | No aurora gradients anywhere | Category convergence trap. Restraint is the differentiator; the pulse is the magic. |
| 2026-05-08 | All-mono UI chrome (buttons, labels, badges, nav, headers) | Reinforces operational software posture. Most devtools use mono only for code; using it for chrome is a deliberate brand signal. |
| 2026-05-08 | Wake-pulse motion is the only signature animation | The metaphor is enacted (live entities pulse) rather than illustrated (no skulls, no Halloween palette). |
| 2026-05-08 | 4px base unit | Engineers want information density. 8px reads SaaS-marketing. |
| 2026-05-08 | Light mode is secondary, never the brand's hero shot | Devtools category baseline is dark; the brand's first impression must be dark. Light mode is a polite afterthought. |

---

## Preview reference

The first rendered preview of this system lives at:
`~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html`

It uses JetBrains Mono as a visual stand-in for Commit Mono (cross-environment reliability). Production system uses Commit Mono.
