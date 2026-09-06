# agentsfleet design system

**Direction:** Clear Signal · **Updated:** 2026-09-05

This document governs the app, website, and shared component library.
Clear Signal names the visual direction; agentsfleet remains the product name.
The website gallery demonstrates the components.

## Intent

Make work, evidence, and next actions easy to recognize.
Keep the mint identity. Use flat surfaces, readable typography, and precise alignment.
Brightness comes from foreground contrast and deliberate color.

The memorable impression is an active fleet whose work you can follow.
Marketing explains outcomes with product examples.
The app prioritizes the current task and the next useful action.

## Typography

| Role | Family | Use |
|---|---|---|
| Display | Bricolage Grotesque | Website hero and large section headings |
| Interface and reading | Instrument Sans | App titles, navigation, buttons, forms, tables, and prose |
| Technical | Commit Mono | Code, logs, identifiers, timestamps, and technical values |

Fonts are bundled through Fontsource. Runtime font downloads from third-party domains are unnecessary.
Use `font-display`, `font-sans`, and `font-mono` through the shared theme.
Do not redefine a font family in a consumer stylesheet.

| Token | Size | Role |
|---|---|---|
| display-xl | 72px maximum | Responsive website hero |
| display-lg | 40px maximum | Website sections |
| display-md | 28px | App titles and major values |
| heading | 20px | Working sections and card headings |
| body-lg | 18px | Website introductions |
| body | 15px | Default reading and controls |
| body-sm | 14px | Supporting copy and navigation |
| eyebrow / label | 12px | Short metadata and section labels |
| mono | 13px | Technical content |

Use sentence case for controls and navigation.
Uppercase is reserved for short eyebrows and compact status labels.
Use tabular numerals for changing values and aligned numeric columns.
Do not use mono for an entire table when its rows contain names and descriptions.

## Color

Dark is the primary brand presentation. Use lifted graphite surfaces and clear silver secondary text; avoid near-black page fills. Light has equal usability requirements.
The token file owns exact values; the following roles explain their use.

| Role | Dark | Light |
|---|---|---|
| Page | Graphite | Cool off-white |
| Card | Lifted graphite | White |
| Input / secondary surface | Distinct graphite layer | Pale green-gray |
| Hover / selected surface | Brighter graphite | Muted green-gray |
| Primary text | Near-white | Deep green-black |
| Secondary text | Clear silver | Dark gray-green |
| Brand action | Bright mint with dark text | Solid ink with white text |
| Link / live signal | Bright mint | Deep teal |

Use semantic foreground tokens on their intended backgrounds.
Check primary, muted, and subtle text against every surface where they appear.
Normal text must reach 4.5:1 contrast; focus and control boundaries must remain identifiable.
Never reduce a readable text token through opacity to create secondary copy.

Separate brand actions from status.
Mint identifies primary actions, links, selected navigation, and live signals.
Success, warning, error, information, and evidence colors communicate their named meanings.
Status always includes text or an icon; color alone carries no required information.

## Surfaces and layout

- Use solid fills. No linear, radial, conic, or mesh gradients, including avatars and usage bars.
- Remove ambient glow fields, text shadows, and glowing buttons.
- Borders define panels. Shadows are reserved for dialogs, menus, and other floating surfaces.
- Use 6px, 8px, and 12px radii for small details, controls, and panels.
- Circular shapes are reserved for avatars and status dots.
- Use the shared 4px spacing scale.
- Keep related controls close and separate different tasks with clear section gaps.
- Avoid cards nested inside cards when a divider communicates the same grouping.

App layouts use consistent navigation, page titles, section headings, and action positions.
PageLayout owns section spacing. PageHeader owns the title and description.
SectionHeader places an action beside the working area it affects.
Descriptions sit below titles and wrap naturally.

Website layouts use an editorial grid within the shared content width.
Pair a concise explanation with a concrete product example.
Vary section composition according to its content.
Keep narrow-screen reading order meaningful without relying on visual placement.

## Components

| Component | Rules |
|---|---|
| Button | Sans label, solid fill, visible focus, stable disabled state |
| Input and textarea | Sans by default; explicit mono only for code or technical data |
| Navigation | Readable sans labels, clear selected state, consistent icon size |
| Tabs | Shared underline style for both route and local tabs |
| Card | Flat surface, fine border, one clear heading |
| Table | Sans names and descriptions; mono identifiers; aligned numeric values |
| Badge | Short readable label; semantic color only when meaningful |
| Dialog | Clear title, focused first action, keyboard dismissal when safe |
| Empty state | Explain the missing item and provide the next action |
| Error state | Explain what failed and retain the recovery action |
| Usage bar | Solid mint fill with visible numeric context |
| Transcript | Distinguish operator input, fleet response, and external activity through structure |

Existing component behavior remains authoritative.
Presentation changes must preserve keyboard interactions, loading state, and accessible names.
Use shared primitives before adding consumer markup with equivalent behavior.

### Tables and record dates

Record lists place Created after the record identity and before Actions.
Secrets and API Keys keep Created visible on narrow screens; the table scrolls when columns need more room.
Event and runner activity feeds keep their primary Time column first.
Do not add creation dates to catalog tables whose records have no useful creation-date field.

Column labels and cells share horizontal padding and alignment.
Left-align dates beneath Created, including secondary usage details in the same cell.
Right-align numeric columns and row actions. Place a numeric column’s sort icon before its label.
Use shared DataTable sorting, pagination, and scroll behavior.

### Forms and popups

Inputs have a visible resting boundary, a mint hover boundary, and a two-pixel focus ring.
Pointer hover must preserve the focus ring on an already focused input.
Dialogs use the shared panel radius, strong border, and overlay token.
The close control uses Button and reserves space beside the dialog title.
Constrain the panel to the viewport and scroll its content so footer actions remain reachable.
Shared buttons provide a minimum 44-pixel touch target on coarse pointers.

Clerk sign-in, sign-up, account menus, and profile forms use `lib/clerkAppearance.ts`.
This adapter maps shared tokens through Clerk’s typed Core 3 appearance API, including `theme`, `options`, and `variables`.
Do not create separate authentication colors or font families.
Clerk’s primary actions, input states, popup backdrop, and corner radii follow the same component roles.
Generated avatar images use the local flat fallback; uploaded user photographs remain visible.
Inspect third-party pseudo-elements for gradients after dependency upgrades.

### Ownership and enforcement

Change font families and light/dark color values in `ui/packages/design-system/src/tokens.css`.
Forward token roles to utilities in `theme.css`. Shared components own their typography,
surface, focus, disabled, and selected styles. A component must render correctly without
consumer CSS; `UsageBar` owns its fill, and `NavItem` owns destination styling.

Pages compose layout and data. Use component variants for visual differences.
Do not copy a component's visual class string into a page or override its font or color
to create a local theme. Add a shared variant when an actual product state requires one.
Interface eyebrows use sans. Apply mono to the technical value itself, never its surrounding
navigation, explanatory prose, or whole table. Keep route labels and recovery actions sans.

`audits/design-tokens.sh` checks named utilities, flat fills, and consumer font ownership.
Component tests verify defaults and state behavior.
`ui/packages/app/tests/interface-typography.test.ts` checks every app page and component
for font overrides on shared interface primitives, including aliased imports and named classes.
Browser review checks both themes,
narrow screens, and loading, empty, error, selected, and disabled states. Static checks
cannot determine whether arbitrary prose is a technical value; that remains a review duty.

## Motion

Use brief color transitions for hover, focus, and selection.
Do not move cards or whole pages when navigating or hovering.
The live indicator may pulse only when the underlying entity is live.
Reduced motion leaves a static, readable state without dimming the content.
Terminal demonstrations may reveal lines when their final content remains available without animation.

## Website illustrations

Use original product illustrations alongside concrete interface examples.
The hero leads with incident response and a small vector teammate. The detailed workflow belongs in How it works.
Supporting diagrams explain an example run and the context available to the next run.
Use recognizable tool marks where they clarify evidence sources or delivery destinations.
The incident diagram distinguishes diagnosis from repair. A human request or failed workflow starts the separate approval-gated repair path.
Label example activity explicitly. Do not present it as live customer activity.
Keep illustrations still. Supporting path animations play once and respect reduced motion.
Use solid color regions and crisp edges. Avoid gradients, glossy shading, and ambient glow.

The shared vector is `ui/packages/website/src/components/AgentIllustration.tsx`.
The large workshop raster is removed. Incident Response and Slack Teammate are the first two showcased workflows.
The website JavaScript budget is 120 kB gzip; the CSS budget remains 20 kB.

## Product copy and pricing

Explain what a fleet does, what it reads, and what the user can inspect.
Keep backend implementation details out of product instructions.
Retain explicit approval requirements wherever an action requires approval.

The website invites early users to try real workflows and provide feedback before launch pricing is decided.
Explain fleet runtime and model usage separately, including that a personal model key does not remove runtime costs.
Do not publish unapproved prices, starter-credit promises, savings estimates, or unlimited free usage.
State that early-access terms precede use and pricing is confirmed before paid usage.
The authenticated app still displays actual balances and charges from its existing billing data.
Do not add subscriptions, payment methods, or automatic reload controls without supporting behavior.
Enterprise discussions must distinguish offered capabilities from requested arrangements.

Address both audiences without separate homepages. Founders need concrete jobs and visible results; infrastructure leads need access boundaries, evidence, and spending controls.
Show setup requirements after the product example. Do not promise instant setup or automatic access from a waitlist submission.

## Interaction principles

Expressed intent proceeds without redundant confirmation.
Credentials and prerequisites appear at the point of need when supported.
Loading, success, and failure states remain visible where the action began.
Destructive actions retain their existing confirmation behavior.

## Implementation and verification

1. Define values in `ui/packages/design-system/src/tokens.css`.
2. Forward named utilities in `ui/packages/design-system/src/theme.css`.
3. Apply roles inside shared primitives.
4. Migrate explicit consumer styles.
5. Inspect app and website in dark, light, narrow, keyboard, and reduced-motion states.

Use the existing design-system gallery as the visual reference.
Keep token mappings, accessible contrast, and meaningful component behavior covered.
Run focused UI checks during implementation.
Run full repository unit and integration suites immediately before the PR, as directed by the user.

## Scope

This update governs web surfaces.
Terminal output keeps its existing palette and rendering rules.
Historical decisions below record prior directions; the current sections above supersede conflicting visual guidance.

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
| 2026-07-07 | Sanction one non-`--pulse` decorative pattern: the account avatar | M119 §5 introduced a deterministic account-avatar pinwheel. Clear Signal retired this exception on 2026-09-05; avatars now use one deterministic solid color. This row remains only as history. |
| 2026-07-22 | Give each Fleet a deterministic robot sigil and agent callsign | The Fleet wall needed persistent identity without adopting friendly mascots or obscuring functional names. The immutable fleet id seeds mirrored geometry and a stable callsign; live Fleets alone use the existing pulse colour and wake ring. The tile also states that a Fleet is an AI agent and exposes a visible Manage fleet affordance. |
| 2026-07-23 | Fleet detail supports an operational conversation | Operators can steer a fleet in a centered transcript alongside evidence from GitHub, Slack, Zoho, Grafana, logs, and other sources. Human turns are distinct from source-context cards; fleet replies remain evidence-first and never use generic consumer-chat styling. |
| 2026-09-05 | Clear Signal: sans interface, expressive display, flat surfaces | User approved brighter clarity, retained mint, and removal of gradients across app and website. |

The flat-fill rule is enforced by `audits/design-tokens.sh` across production app, website, and shared design-system CSS and TypeScript sources.
