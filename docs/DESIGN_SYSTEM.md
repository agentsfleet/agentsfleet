# Design System — Operational Restraint

**Version:** 0.1 · 2026-05-08
**Source of truth.** All visual, typographic, and motion decisions in `ui/packages/website`, `ui/packages/app`, `ui/packages/design-system`, `docs.agentsfleet.net`, and `agentsfleet` output read from this document. Do not deviate without explicit user approval and a corresponding update here.

---

## Memorable thing

**"It wakes."** A long-lived daemon that wakes on events, runs against a durable replayable log, receives operator direction, and posts evidenced answers. Every visual decision serves this posture.

---

## Product context

- **What:** Always-on operational runtime. Agents are long-lived daemons that own one operational outcome end to end.
- **Who for:** Engineers running production infrastructure who want events → evidence → diagnosis without wiring a chatbot.
- **Category:** Developer infrastructure / observability adjacent.
- **Surfaces:**
  - `ui/packages/website` — marketing site (`agentsfleet.net`)
  - `ui/packages/app` — authenticated product UI (`app.agentsfleet.net`)
  - `ui/packages/design-system` — shared React component library
  - `docs.agentsfleet.net` — long-form technical documentation
  - `agentsfleet` — CLI output (rendered in 256-color terminals)

---

## Aesthetic direction

**"Operational Restraint"** — serious infrastructure brand language with one signature of liveness nobody else owns.

- **Reference vibes:** Anthropic console × Datadog × a single bioluminescent pulse.
- **Anti-vibes:** Vercel/Linear aurora gradients, purple-to-blue meshes, "magical" hero animations, friendly mascots, generic consumer-chat bubbles, decorative blobs, gradient CTA buttons, bubble-radius everything.
- **Decoration level:** minimal. The mono typography + the pulse do all the work. A subtle dot-grid background is permitted on marketing hero only (8% opacity).
- **Mood:** evidenced, machine-precise, slightly haunted, never decorative. The product feels alive but never performs.
- **Differentiation strategy:** restraint as the differentiator. Every competitor uses aurora gradients. By having none, the single pulse color owns all attention.

---

## Interaction restraint — minimize end-user friction

Restraint is procedural, not only visual. Default every flow to the **fewest steps the user must take** — friction is debt, justified only by real risk.

- **No confirmation beats** where intent is already expressed. A primary action (e.g. "Use template") *is* the commit; don't gate it behind a second "Confirm." **Auto-proceed** once prerequisites are met.
- **Resolve in place.** When a flow needs a credential or a value, surface the input **inline at the point of need** — never bounce the user to another page and back.
- **Auto-resume.** The instant a gate is satisfied, the flow continues on its own; the user never re-initiates.
- **Show, don't ask.** Push live state (the run is provisioning) rather than making the user poll, refresh, or click "check status."

**The one exception:** destructive or irreversible actions still confirm. Cutting friction never overrides the safety-confirm rule.

## Product copy

App copy is short, literal, and useful at scan speed.

- **Page subtitles:** one short sentence when possible; two short sentences only when the second carries state the user needs now.
- **Helper text:** explain the next action, not the feature. Prefer "Create a workspace first." over longer prerequisite prose.
- **Credential copy:** always say write-only, but keep it short: "Write-only. Replace to rotate."
- **Install copy:** keep the three beats visible: pick source, connect token, watch live states.
- **Integration copy:** show current status and one action. Do not describe roadmap detail in row subtitles.

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
| `--surface-1` | `#141A1F` | Default elevated surface (cards, sidebars). Lifted from `#11161A` on Jul 07, 2026 — see Decisions log. |
| `--surface-2` | `#181E22` | Inputs, mockup chrome, elevated cards. |
| `--surface-3` | `#1F262C` | Hover state, more-elevated layer. |
| `--border` | `#2B333A` | Default borders. Lifted from `#23292E` on Jul 07, 2026 — see Decisions log. |
| `--border-strong` | `#2E373E` | Active/focused borders, button outlines. |
| `--text` | `#E6EAEC` | Default text. Off-white, never pure `#FFF`. |
| `--text-muted` | `#8B9398` | Secondary text, captions. |
| `--text-subtle` | `#7A8085` | Tertiary text, timestamps, dim CLI output. AA against `--bg` (4.88:1); lifted from `#5C6469` (3.23:1) on May 11, 2026. |

### The pulse — used only on live signals

| Token | Hex | Rule |
|---|---|---|
| `--pulse` | `#5EEAD4` | **Bioluminescent cyan-mint.** The signature accent. Used **only** on live/awake/wake signals: pulse rings on running agents, `LIVE` badges, the brand-mark dot, primary CTA buttons, link color, focus rings. Treat as currency — every additional use dilutes. |
| `--pulse-dim` | `#2DD4BF` | Pressed state for primary buttons; pulse desaturated. |
| `--pulse-glow` | `rgba(94, 234, 212, 0.35)` | The expanding ring color in the wake-pulse keyframe. |

**Forbidden uses of `--pulse`:** decorative borders, large background fills, gradient stops, hover states on non-live elements, illustrations.

**Sanctioned non-pulse exception — the account avatar:** the dashboard's account-avatar fallback (Clerk `UserButton`, no uploaded photo) is the one place a non-`--pulse` decorative gradient is allowed: a deterministic two-colour `repeating-conic-gradient` pinwheel, hashed from the signed-in user's id (hue, second hue, and start angle all derived from the hash), so each account reads as visually distinct. Two colours only (within the "no three-or-more-stop gradients" rule below); never `--pulse` as one of them. Added Jul 07, 2026 — see Decisions log.

**Fleet identity sigils:** every Fleet wall tile carries a deterministic, mirrored dot-matrix robot sigil and agent callsign derived from the immutable fleet id. The geometry and callsign provide identity without replacing the Fleet's functional name or storing profile data. Resting sigils use surface, border, and muted-text tokens; only an actually-live Fleet may switch the sigil to `--pulse` and inherit the existing wake ring. The sigil never introduces a second accent, decorative card border, or permanent glow.

### Status (use sparingly)

| Token | Hex | Use |
|---|---|---|
| `--success` | `#34D399` | Success log lines, OK states, deltas trending good. |
| `--warn` | `#F59E0B` | Degraded agents, warning logs. |
| `--error` | `#F87171` | Failed agents, error logs. |
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
| `--text-subtle` | `#67706B` | AA against light `--bg` (4.73:1); darkened from `#8A918A` (2.99:1) on May 11, 2026. |
| `--pulse` | `#14B8A6` | Pulse desaturates 15% in light mode. |
| `--pulse-dim` | `#0D9488` | |
| `--pulse-glow` | `rgba(20, 184, 166, 0.30)` | |
| `--ink` | `#17211F` | Light-mode primary-CTA fill (solid ink, white text). Mint stays for accents/links/active/glow. |

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
- **CLI / agentsfleet:** 256-color palette mirroring web tokens. Pulse cyan for live state, amber for `EVIDENCE` lines, status colors restrained, no decorative ASCII art (no boxes-around-titles, no banners, no ASCII agents).
- **Max content width (marketing & docs):** 1280px.
- **Border radius:** small and hierarchical. `--r-sm: 6px`, `--r-md: 9px`, `--r-lg: 14px`. **No `border-radius: 9999px` on buttons. Ever.** Only on circular dots, avatars, status rings.
- **Borders preferred over shadows.** A `1px solid var(--border)` is the default elevation cue. Drop shadows are only for floating elements (popovers, modals).

---

## Motion

The system has **two** signature animations: the wake pulse (live-signal, system-wide) and the install-demo terminal reveal (marketing hero only). Beyond those, **marketing and docs stay still** — motion is functional or absent. The **operator dashboard** (the gated app) is the one deliberate exception: a lived-in workspace earns a restrained, fully reduced-motion-gated motion pass — content mount-rise, a slow ambient glow-drift, hover/press micro-interactions — see *Dashboard motion pass* below.

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
- Fires only on actually-live entities (running agents, active streams, `LIVE` badges, the brand-mark dot in the header, the cursor on the hero).
- The instant a agent is parked, the animation stops.
- Failed/degraded agents get a static ring in their status color; no pulse.
- Maximum on-screen: ~5 simultaneous pulses. More than that is visual noise; consolidate to a count.

### Install-demo terminal reveal (marketing)

The marketing hero's `<Terminal animate>` reveals its install transcript
line-by-line (staggered `animation-delay`, opacity-only — no slide) so the demo
reads as "running live". CSS-driven (`[data-terminal-reveal]` in tokens.css),
no JS timer — same discipline as the wake pulse.

**Rules:**
- Marketing surfaces only. Operational log streams (the dashboard event log)
  keep the functional 80ms fade below — no stagger there.
- One-shot: lines hold their final visible frame (`forwards`); the reveal never loops.
- `prefers-reduced-motion: reduce` → every line visible at once, no reveal.

### Dashboard motion pass (operator app)

The gated operator dashboard is the one surface that performs — a restrained
layer over the static system, scoped to `ui/packages/app` and defined in
`app/globals.css`. It exists because the dashboard is a lived-in workspace, not
a one-shot marketing read; a little life reads as responsive without tipping
into the anti-vibes traps the rest of this doc forbids.

- **Mount-rise:** page content fades + rises 10px (`rise-in`, 0.38s, no
  overshoot) as it mounts / on route change, gently staggered across the page's
  top-level sections. `both` fill resolves to the visible frame — never pins
  `opacity: 0` as a resting state.
- **Ambient glow-drift:** the dashboard's single `--pulse` glow drifts a few
  percent over ~24s (`glow-drift`). Low-opacity, single radius — a slow breath,
  explicitly **not** an aurora/mesh gradient.
- **Micro-interactions:** a faint brightness lift on button hover, a 1px press
  on active, a 1px sidebar-nav nudge on hover. Transform/filter only — the
  colour transitions stay with the design-system primitives.

**Rules:**
- Scoped to the gated app dashboard. Marketing + docs keep the "instant, no
  performance" restraint below.
- **Every effect is gated:** keyframe animations are neutralised by the global
  `prefers-reduced-motion: reduce` block; the hover/press lifts live inside a
  `no-preference` query; the nav nudge uses the `motion-safe:` variant. Under
  reduced motion the dashboard is as still as the rest of the system.
- Pinned by `app/tests/shell-motion.test.ts` — the gate is structural, not a
  review note.
- Later dashboard motion extends this pass under the same reduced-motion
  guarantee (the Billing balance meter-fill lands with that screen's rebuild).

### Functional motion

- **Hovers:** `transition: 50ms ease-out`. Snap. No bounce, no spring, no `cubic-bezier` overshoot.
- **Focus rings:** instant, no animated draw-on. `box-shadow: 0 0 0 3px var(--pulse-glow)`.
- **Page transitions:** instant on marketing + docs — no fade, no slide. The gated operator dashboard is the exception: page content performs a restrained mount-rise (≤0.4s, reduced-motion-gated) on route change — see *Dashboard motion pass* above.
- **Log streams (operational):** new lines fade in over 80ms (`opacity 0 → 1`). No slide-up, no stagger. (The marketing install-demo terminal is the one sanctioned staggered reveal — see above.)
- **Loading states:** prefer skeleton bars (1-pixel-thick borders) when the page shape is still resolving. For active work already underway (button submits, install states, short route waits), use the shared `Spinner`: a tiny monochrome arc around the WakePulse dot. When visible text is needed, render it as the compact mono install chip (`rounded-md`, pulse-tinted border/background, label text). Do not introduce page-local loader glyphs.

### `prefers-reduced-motion: reduce`

- Pulse animations become a static ring at `0.2` opacity (`box-shadow: 0 0 0 4px var(--pulse-glow)`).
- Log-stream fade disabled; the install-demo terminal reveal shows all lines at once.
- Hover transitions retained at 50ms (functional, not decorative).

### Forbidden motion

- Bouncy easings (`elastic`, `bounce`, anything with overshoot)
- Page-transition fades or slides **on marketing + docs** (the gated dashboard mount-rise is the one sanctioned exception — *Dashboard motion pass*)
- Scroll-driven animations on marketing (other than the static dot-grid)
- Animated gradients **as decoration** — no aurora/mesh gradients anywhere; the dashboard's slow, low-opacity single-radius glow-drift is the one sanctioned exception
- Cursor-following effects, parallax, mouse-tracking glows
- Spring physics on UI chrome

---

## Component principles

- **Buttons:** mono font, 13px, padding `12px 16px`, border-radius `--r-md` (`9px`). Three variants: `primary` (fills with `--cta` — the pulse in dark, **solid ink in light**; `--cta-foreground` text: dark on mint, white on ink), `default` (surface-2 fill, border-strong outline), `ghost` (transparent, muted text). No gradient buttons. No icon-only buttons larger than 36px square.
- **Badges:** mono font, 11px, padding `4px 8px`, border-radius `--r-sm`. Status badges (`LIVE`, `degraded`, `failed`) get colored fills; informational badges get muted outlines.
- **Form fields:** surface-2 background, border on default, pulse-cyan focus ring with `--pulse-glow` shadow. Mono font for input values (they're operational data, not prose).
- **Fleet transcripts:** a centered reading column makes human, fleet, and external-source turns scannable. Operator turns may align right in a restrained bordered surface; fleet replies stay open and left aligned; integration turns are compact source-context cards with their outcome below. This is a conversation with operational evidence, not a generic consumer chat.
- **Cards:** surface-1 background, 1px border, `--r-lg` (`14px`) radius. Padding 24px default, 16px in dense data views.
- **Tables / lists:** prefer flat rows with 1px bottom borders over zebra-striping. Tabular-nums everywhere. Right-align numbers, left-align text.
- **Sidebars:** surface-2 background. Mono nav items, 12px. Active item gets surface-3 fill, not a colored bar.
- **Tabs:** one visual — an underline. Inactive triggers read `--text-muted` on a thin `--border` rail; the active trigger lights its 2px bottom-border to `--pulse` (a sanctioned "active" use of the currency) and its label to `--text`. No pill tray, no `bg-background` active fill, no shadow. The in-page Radix tabs and the route-style tab-nav share one style module (`design-system/tab-styles.ts`).
- **Dashboard page rhythm:** `PageLayout` owns the 32px gap between direct page sections. It uses flex gap, never adjacent margins, so standard and full-height pages share the same spacing. `PageHeader` owns title and description only. `SectionHeader` owns the labelled working area and places its primary action on the right when that area has one.
- **Page header:** the page title sits on its own line; a one-line **description renders directly below it** (muted, body-sm), never beside it. An optional page-level action pins top-right only when it acts on the whole page. The title + description form a left column, and the action aligns to its top.
- **Usage bars:** a thin (8px) full-width track (`--surface-3`/`bg-accent`) with a `--pulse-dim → --pulse` gradient fill whose width is the consumed fraction; the fill animates `0 → value` on load (`meter-fill`, reduced-motion-gated). A usage bar, not a gauge — an optional label + tabular-nums percentage row sits above, an optional caption below. `UsageBar` (`design-system/UsageBar.tsx`), not a marketing primitive; `globals.css` owns only the animation keyframe. First consumer: Billing's balance card (unlabeled — the dollar headline above it already states the value).
- **Option cards:** a bordered choice card (icon slot + label + optional one-line description), `data-state="checked"` gets a `--primary`/`--border-strong` ring — the picker idiom for a small (2-5) set of mutually-exclusive choices where a plain dropdown hides the tradeoff. Built on the existing `RadioGroup`/`RadioGroupItem` Radix primitive (`OptionCard`, `design-system/OptionCard.tsx`), not a second radio implementation. First consumer: `AddRunnerDialog`'s isolation-mode field, replacing a `Select` dropdown.

---

## CLI / agentsfleet rendering

- **Palette mapping** (256-color terminal):
  - `--pulse` → `#5EEAD4` (closest 256: 79 / `cyan2`)
  - `--evidence` → `#FBBF24` (closest 256: 220 / `gold1`)
  - `--success` → `#34D399` (closest 256: 78)
  - `--warn` → `#F59E0B` (closest 256: 214)
  - `--error` → `#F87171` (closest 256: 210)
  - `--text-muted` → `#8B9398` (closest 256: 102 / `grey53`)
  - `--text-subtle` → `#7A8085` (closest 256: 244)
- **Status glyphs:**
  - Live: `●` in `--pulse`
  - Parked: `○` in `--text-subtle`
  - Degraded: `●` in `--warn`
  - Failed: `✕` in `--error`
- **`EVIDENCE` lines:** `EVIDENCE` label in `--evidence`, source ref in `--text`, quoted content in `--text-muted`.
- **No decorative ASCII art** — no agent face, no boxes around titles, no banners, no rocket emoji. The CLI is operational output.

---

## Implementation roadmap (separate effort from this doc)

This document is the spec. Implementation is a separate milestone. Suggested workstream split:

1. **W1 — `ui/packages/design-system`:** rewrite `tokens.css`, `theme.css`. Swap `@fontsource-variable/geist` for Commit Mono (self-host) + Instrument Sans (Google Fonts or self-host). Update every component (Button, Badge, Card, Input, etc.) to read new tokens. Add `<WakePulse />` primitive for the signature animation.
2. **W2 — `ui/packages/website`:** apply new tokens, replace any Geist references, rebuild marketing hero with the new typography scale, add the dot-grid hero background.
3. **W3 — `ui/packages/app`:** apply new tokens, audit every page against the dashboard mockup, ensure `<WakePulse />` only fires on actually-live agents (data-driven, not decorative).
4. **W4 — `docs.agentsfleet.net`:** apply new typography stack, single-column layout, ~68ch measure.
5. **W5 — `agentsfleet`:** add 256-color terminal mode (detect via `tput colors`), implement status glyphs, audit every output line for the new palette mapping.
6. **W6 — Wire-up:** add a `docs/DESIGN_SYSTEM.md` row to the EXECUTE doc-reads table in `AGENTS.md` (triggers: `*.tsx`, `*.css`, files under `ui/packages/**`, `cli/src/**` when touching output formatting). Triggers the Invariance Suite Gate — handle as its own commit.

Each workstream is its own spec. Use `kishore-spec-new` to create them once you're ready to start implementation.

---

## Decisions log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-08 | Initial design system created — Operational Restraint direction | Created via `/design-consultation`. User picked dark-primary, operational mono display, restrained agent metaphor (one pulse signal), single bioluminescent accent. Memorable thing locked as "It wakes." |
| 2026-05-08 | Drop Geist (currently in `ui/packages/website`, `ui/packages/app`) | Overused; the new Inter. Replaced with Commit Mono + Instrument Sans. |
| 2026-05-08 | No aurora gradients anywhere | Category convergence trap. Restraint is the differentiator; the pulse is the magic. |
| 2026-05-08 | All-mono UI chrome (buttons, labels, badges, nav, headers) | Reinforces operational software posture. Most devtools use mono only for code; using it for chrome is a deliberate brand signal. |
| 2026-05-08 | Wake-pulse motion is the only signature animation | The metaphor is enacted (live entities pulse) rather than illustrated (no skulls, no Halloween palette). |
| 2026-05-08 | 4px base unit | Engineers want information density. 8px reads SaaS-marketing. |
| 2026-05-08 | Light mode is secondary, never the brand's hero shot | Devtools category baseline is dark; the brand's first impression must be dark. Light mode is a polite afterthought. |
| 2026-05-11 | Lift `--text-subtle` to ≥4.5:1 WCAG AA in both themes | Audit found the pre-existing values failed body-text AA (dark 3.23:1, light 2.99:1). Tertiary text + CLI subtle output + eyebrow labels were borderline-illegible at small sizes. Dark `#5C6469 → #7A8085` (4.88:1); light `#8A918A → #67706B` (4.73:1); CLI xterm256 `240 → 244`. |
| 2026-05-21 | Add a second sanctioned animation: the marketing install-demo terminal reveal | Supersedes the 2026-05-08 "only signature animation" call for marketing surfaces. The hero shows the install "running" via a one-shot staggered line reveal (opacity-only, CSS-driven, reduced-motion-safe). Scoped to marketing; operational log streams stay non-staggered. |
| 2026-06-23 | Add "Interaction restraint — minimize end-user friction" principle | Indy: "always think about adding less friction to an end user." Restraint is procedural, not only visual — auto-proceed once intent is expressed (no confirm beats), resolve inputs inline, auto-resume on gate satisfaction, push state instead of poll. Surfaced from the M98 install-fleet flow (auto-create after import/gate). Destructive actions still confirm. |
| 2026-06-23 | Retire the pill tab; one underline tab style (app) | M98 §1.1. The app shipped two tab visuals (a `bg-muted` pill tray with a `bg-background` active fill) applied inconsistently. Unified to a single underline: active = a `--pulse` 2px bottom-border on a `--border` rail; inactive `--text-muted`. Shared `design-system/tab-styles.ts` consumed by `Tabs` (Radix) + `TabNav` (links) + their tests (RULE UFS). Approved in the M98 mockup (`docs/design/M98_001-ui-polish-preview.html`). |
| 2026-06-23 | Add a scoped motion pass to the operator dashboard (mount-rise, ambient glow-drift, hover/press micro-interactions) | M98 §1.5. A lived-in operator dashboard performs where marketing stays still — approved by Indy as full mockup motion (`docs/design/M98_001-ui-polish-preview.html`, signed off screen-by-screen). Supersedes the 2026-05-08 "everything else functional or absent" call and the §Motion "page transitions instant / operational software does not perform" rule **for the gated app dashboard only**; marketing + docs keep the restraint. Every effect is reduced-motion-gated; pinned by `app/tests/shell-motion.test.ts`. |
| 2026-06-23 | Billing reads consumption-honest: balance + meter, a terminal usage ledger, one "Pay as you go" row — no seat grid | M98 §2. Consumption/prepaid billing has no seats, so the seat-plan grid is dropped for a single honest Current row + a volume-pricing link. The balance card leads with amount + a full-width usage meter; usage history is a terminal-native `date · amount · type · description` ledger (model + tokens fold into the description, since the telemetry has no free-text field). Presentation only — the billing data path is unchanged. |
| 2026-06-24 | Split Models and Credentials into two destinations; option-card mode picker; `/credentials` is a real vault | M98 §3–§4. The conflated "Models & Credentials" tabbed page becomes two nav entries. Models collapses its triple "current setup" restatement into **two option-cards** — the active one badged "Current" reading "Active — nothing to do" (no button), the action living only on the option you'd switch *to*. `/credentials` is a write-only **vault**: a kinds strip (Model providers · Custom secrets · Integrations) then those groups in order. Model-provider rows add or replace Anthropic/OpenAI keys in place; custom secrets show Added + Replace; GitHub is native, while Zoho/Slack show Planned + Request access to capture demand. |
| 2026-06-24 | Install is minimal and state-driven — no review page | M98 §9. The Dashboard previews template cards; the Fleets empty-state offers one Install fleet action; `/fleets/new` owns the full source picker (template grid · `owner/repo` · paste-SKILL.md). One click proceeds **inline** through terminal-native install states (importing → connect-to-continue → creating → done; errors retry) — the `BundlePreview` review page is removed. Create **auto-proceeds** (no confirm beat). Live status reuses the existing Server-Sent Events (SSE) fleet-event stream (no polling); an installing fleet always shows its state; "Open fleet" lands in the full-height steer/chat. |
| 2026-06-23 | Page header: description renders below the title | M98 §1.2. `PageHeader` gained a `description` slot (muted body-sm, stacked under the title) + an optional top-right `actions` slot; the bare flex-row shape stays back-compatible. Fixes the description-beside-title drift (the app was rendering the page description as a right-aligned sibling). |
| 2026-06-23 | Light-mode primary CTA = solid ink (not mint) | M98 §1.4. Added a `--cta` token isolated from `--pulse`: dark = the pulse, light = solid ink (`--ink` `#17211F`, white text). Keeps mint as currency (accents/links/active/glow) while the light-mode primary button reads as confident ink. `Button` default variant consumes `--cta`/`--cta-foreground`. |
| 2026-07-07 | Lift dark-mode `--border`/`--surface-1` one step brighter | M119 §1. Resting-state cards/tables only read as defined on hover (`--border` sat ~4% luminance above `--surface-1`), benchmarked against a PlanetScale dashboard reference. Dark `--border` `#23292E → #2B333A`, `--surface-1` `#11161A → #141A1F`. Value-only; every existing `border-border`/`bg-card` consumer inherits it. Light mode untouched (vestigial — `ThemeToggle.tsx` forces dark). |
| 2026-07-07 | Formalize `UsageBar` and `OptionCard` as shared primitives | M119 §2, §4. `UsageBar` extracts the bespoke `.app-meter` markup (previously hand-rolled once, in `BillingBalanceCard`) into a reusable component — see "Usage bars" above. `OptionCard` builds the M98 §3-4 "option-card" idiom (until now ad-hoc prose, never extracted into code) on top of the existing, previously-zero-consumer `RadioGroup` primitive — see "Option cards" above. First consumer: `AddRunnerDialog`'s isolation-mode field. |
| 2026-07-07 | Sanction one non-`--pulse` decorative pattern: the account avatar | M119 §5. The dashboard account avatar (Clerk `UserButton` fallback) rendered every user against the same flat `--surface-2`. Added a deterministic, per-user `repeating-conic-gradient` pinwheel (hue, second hue, and start angle all hashed from the user id) so accounts read as visually distinct — a pattern reads closer to "distinct identity" than a smooth blend, approximating GitHub/Linear-style per-account avatar colour without a pixel-grid identicon (Clerk's `appearance.elements` styling hook accepts CSS values only, not custom child markup — a true identicon is a follow-up, not this patch). Never `--pulse`; two colours only, within the "no three-or-more-stop gradients" rule. |
| 2026-07-22 | Give each Fleet a deterministic robot sigil and agent callsign | The Fleet wall needed persistent identity without adopting friendly mascots or obscuring functional names. The immutable fleet id seeds mirrored geometry and a stable callsign; live Fleets alone use the existing pulse colour and wake ring. The tile also states that a Fleet is an AI agent and exposes a visible Manage fleet affordance. |
| 2026-07-23 | Fleet detail supports an operational conversation | Operators can steer a fleet in a centered transcript alongside evidence from GitHub, Slack, Zoho, Grafana, logs, and other sources. Human turns are distinct from source-context cards; fleet replies remain evidence-first and never use generic consumer-chat styling. |

---

## Preview reference

The first rendered preview of this system lives at:
`~/.gstack/projects/agentsfleet/designs/design-system-20260508-0831/preview.html`

It uses JetBrains Mono as a visual stand-in for Commit Mono (cross-environment reliability). Production system uses Commit Mono.
