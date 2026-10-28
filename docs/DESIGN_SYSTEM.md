# agentsfleet design system

**Direction:** Clear Signal · **Updated:** 2026-10-18

This document governs the app, website, and shared component library.
Clear Signal names the visual direction; agentsfleet remains the product name.
The website gallery demonstrates the components.

## Intent

Make work, evidence, and next actions easy to recognize.
Keep the approved charcoal, warm-white, and mint identity. Use compact controls, confident type, layered surfaces, and precise alignment.
The interface should feel bold, simple, and colorful enough to distinguish actions and outcomes at a glance.
Brightness comes from foreground contrast, mint action surfaces, and visible semantic accents.
Make the experience visually rich through composition, purposeful motion, and clear surface depth within the existing palette.
Reduce padding and repetition before reducing readable text. Avoid oversized controls and empty panels.

The memorable impression is an active fleet whose work you can follow.
Marketing explains outcomes with product examples.
The app prioritizes the current task and the next useful action.

### Evolving the direction

This document records current design choices. Update it when a better interaction or an explicit owner decision changes those choices.
The 2026-10-18 direction preserves the branding, fonts, and colors approved in Pull Request (PR) #752 and the M227 workstreams.
Modernization changes hierarchy, composition, surface treatment, and motion within that identity.

Gradients and animation are permitted design tools. Generic review preferences must not veto a deliberate, reviewed use.
Derive decorative light from existing color tokens. Keep reading surfaces readable, and retain usable boundaries without decoration.
Prefer one coherent visual treatment across a route family over isolated effects on individual cards.

| Surface | Visual emphasis | Interaction |
|---|---|---|
| New fleet | Centered prompt, signal mark, focused composer | Gentle entrance and a visible typing caret |
| Fleet workspace | Clear section navigation, visible outcomes, strong headings | Brief section arrival; stable transcript while typing |
| Lists and catalogs | Strong identity column, readable status, aligned actions | Responsive hover and focus; stable rows during refresh |
| Settings | Named sections, grouped controls, visible save feedback | Disclose detail near the control that needs it |
| Evidence and configuration | Readable summary with complete details available | Accessible disclosure and bounded scrolling |

## Typography

| Role | Family | Use |
|---|---|---|
| Display | Bricolage Grotesque | Website hero and large section headings |
| Interface and reading | Instrument Sans | App titles, navigation, buttons, forms, tables, and prose |
| Technical | Commit Mono | Code, logs, identifiers, exact timestamps, and technical values |

Fonts are bundled through Fontsource. Runtime font downloads from third-party domains are unnecessary.
Use `font-display`, `font-sans`, and `font-mono` through the shared theme.
Do not redefine a font family in a consumer stylesheet.

Billing's approved compact surface uses the shared `font-system` utility for the platform system font.
Its activity cells use 12px normal text, names and totals use medium weight, and spending rows use 13px normal text.
Align disclosed labels left and values right through DescriptionList on the shared card surface.

| Token | Size | Role |
|---|---|---|
| display-xl | 72px maximum | Responsive website hero |
| display-lg | 40px maximum | Website sections |
| display-md | 28px | App titles and major values |
| heading | 20px | Working sections and card headings |
| body-lg | 18px | Website introductions |
| chat-title | 19px | Fleet and setup header titles |
| conversation | 15px | Fleet and setup transcript, with 1.75 line height |
| body | 15px | Default reading and controls |
| body-sm | 14px | Supporting copy, table cells, and navigation |
| label | 13px | Metadata, table headings, and compact status badges |
| eyebrow | 12px | Short section labels |
| mono | 13px | Technical content and aligned identifiers |

Use sentence case for controls and navigation.
Form labels use 13px semibold sans; normal button labels use 14px semibold sans.
Keep descriptions at normal weight so labels and actions carry a clear hierarchy.
Uppercase is reserved for short eyebrows, compact status labels, and shared agent display names. Table headings use sentence case.
Keep long labels in sentence case so they remain easy to scan.
Use tabular numerals for changing values and aligned numeric columns.
Do not use mono for an entire table when its rows contain names and descriptions.

The 13px step serves compact sans labels, table headings, and technical mono values.
Use 14px for supporting descriptions, operational status lines, and relative times.
Reserve monospace for exact values whose character alignment helps comparison.

App page titles use the shared 28px semibold `PageTitle`; do not restyle individual page headings.
Both fleet entrances use this title style.
Fleet workspace headers use the compact `chat-title` role above the conversation.

## Color

Dark is the primary brand presentation. Use charcoal surfaces, warm off-white text, and restrained mint. Light has equal usability requirements.
The token file owns exact values; the following roles explain their use.

| Role | Dark | Light |
|---|---|---|
| Page | Charcoal | Warm off-white |
| Card | Lifted charcoal | White |
| Input / secondary surface | Distinct charcoal layer | Pale green-gray |
| Hover / selected surface | Brighter charcoal | Muted green-gray |
| Primary text | Warm off-white | Deep green-black |
| Secondary text | Clear gray-green | Dark gray-green |
| Brand action | Bright mint with dark text | Solid ink with white text |
| Secondary action | Mint tint, mint text and border | Teal tint, deep teal text and border |
| Link / live signal | Bright mint | Deep teal |

Use semantic foreground tokens on their intended backgrounds.
Check primary, muted, subtle, mint, and status text against every surface where they appear.
Normal text must reach 4.5:1 contrast; focus and control boundaries must remain identifiable.
Never reduce a readable text token through opacity to create secondary copy.

Test both themes with axe-core in the rendered browser and the shared token contrast tests.
Include badges with tinted backgrounds; a foreground passing on the page can still fail inside a badge.
Inspect incomplete browser results manually when overlays or pseudo-elements prevent a background measurement.

Contrast ratios measure readability, not perceived brightness on every display.
Keep the dark canvas readable and lift working surfaces enough to distinguish content groups.

Use mint light to emphasize an entrance. Chat composers keep a quiet neutral boundary while focused.

Separate brand actions from status.
Mint identifies primary actions, links, selected navigation, and live signals.
Success, warning, error, information, and evidence colors communicate their named meanings.
Status always includes text or an icon; color alone carries no required information.
Success uses a distinct green, while mint remains the action and live-signal accent.
Use information blue for informative state, evidence amber for sourced material, and amber for attention.
Give compact badges a visible tinted surface and boundary. Keep neutral metadata quiet; do not color every label.
Secondary actions and hovered controls use a mint tint instead of another indistinguishable gray layer.

The shared `BrandMark` is a static eight-ray signal with a central dot, paired with
the wordmark. It identifies the product without implying live execution. Use
`WakePulse` only for actual live state. Shared brand assets live in
`ui/packages/design-system/assets/`; website and app icons reference that source.

## Surfaces and layout

- Keep reading surfaces solid. Decorative light may frame a hero or working surface without reducing text contrast.
- Chat composers show focus through the typing caret and a stronger neutral outer border. Avoid a second outline or mint glow.
- Use borders, surface contrast, and shared elevation roles to establish depth. Floating surfaces remain clearly above the page.
- Use 6px, 8px, and 12px radii for small details, controls, and panels.
- Circular shapes are reserved for avatars and status dots.
- Use the shared 4px spacing scale.
- Keep related controls close and separate different tasks with clear section gaps.
- Avoid cards nested inside cards when a divider communicates the same grouping.

App layouts use consistent navigation, page titles, section headings, and action positions.
PageLayout owns section spacing. PageHeader owns the title and description.
PageHeader separates the route introduction from its working area with a fine divider and readable supporting copy.
SectionHeader places an action beside the working area it affects.
Descriptions sit below titles and wrap naturally.
Page headers and their actions wrap on narrow screens.
Keep the full wordmark visible at 320px without competing with account controls.
Below 360px, the workspace picker becomes a labeled 44px icon button; its menu still exposes workspace names.

Website layouts use an editorial grid within the shared content width.
Pair a concise explanation with a concrete product example.
Vary section composition according to its content.
Keep narrow-screen reading order meaningful without relying on visual placement.

## Components

| Component | Rules |
|---|---|
| Button | Compact semibold sans label; solid primary fill, tinted secondary fill, visible focus, stable disabled state |
| Input and textarea | Compact sans fields with strong resting boundary; explicit mono only for code or technical data |
| Navigation | Readable sans labels, clear selected state, consistent icon size |
| Tabs | Shared underline style for both route and local tabs |
| Card | Solid surface, fine border, one clear heading; interactive cards lift and reveal a mint boundary on hover or focus |
| Table | Sans names and descriptions; mono identifiers; aligned numeric values |
| Badge | Short semibold label, compact line height, visible semantic tint and border |
| Dialog | Clear title, focused first action, keyboard dismissal when safe |
| Empty state | Explain the missing item and provide the next action |
| Error state | Explain what failed and retain the recovery action |
| Usage bar | Solid mint fill with visible numeric context |
| Transcript | Distinguish operator input, fleet response, and external activity through structure |

Existing component behavior remains authoritative.
Presentation changes must preserve keyboard interactions, loading state, and accessible names.
Use shared primitives before adding consumer markup with equivalent behavior.

Reusable visual treatments belong in `ui/packages/design-system`. App widgets combine those primitives with product behavior and data.

Use Activity for the workspace feed. Fleet activity appears in its conversation. Individual records remain events; existing event URLs retain their meaning.

The fleet header offers **Edit setup** and **Fleet actions**. **Edit setup** opens the authoring page at the fleet's saved version.

Fleet messages, setup receipts, and integration activity share one chronological conversation.

**Save fleet** sits beside the authoring chat title. Authoring a saved fleet shows **Save changes** in that position.
**Stop** and permanent **Kill** live in **Fleet actions**, with confirmation dialogs.

Setup and saved chat share a 728px reading column and a composer pinned to its bottom edge.
User messages use a quiet bubble; assistant replies use plain prose and the fleet's name.
**Saved memories** opens from **Fleet actions**. The dialog has one title; its description fits on one line on desktop and wraps on narrow screens.

Each tool call has a separate entry with its recorded name, status, and elapsed clock.
A running call has its own spinner. **Input** and **Output** or **Error** open independently for each call.
Commands and returned text stay inside these disclosures. Tool status does not claim the fleet's overall task succeeded.

Create and Edit secret use the same field editor with masked values and add/remove controls.
Both dialogs share the encryption explanation and help link. Edit locks the name and explains that saving replaces every field.
Stored values are never loaded into the editor. Secret names display Copy only; value editing opens from Actions.

### Compact density

Default buttons, text inputs, and select triggers are 36px tall on fine pointers. Small buttons are 32px; large actions are 44px. Icon buttons share the same compact rhythm.
Coarse pointers retain a minimum 44px interactive target. Visible density must not make touch, keyboard focus, or long labels harder to use.
Use 13px form labels and badges, 14px control labels and supporting text, and the existing 15px body scale. Smaller components do not mean tiny type.
Place related controls in one wrapping row where practical. Keep dialogs focused on the current decision and disclose advanced configuration through the shared Accordion.
Use shared Button, Input, Select, Label, Badge, and StatusPill variants for these decisions. Pages must not recreate their appearance with local class overrides.
Use `Card interactive` only when the surface opens a destination. Static information panels keep their resting treatment.
Primary controls press briefly; interactive cards lift by one small spacing step and move their Open cue toward the destination.
These effects use shared styles in `interactions.css`. Reduced motion retains color and focus feedback without movement.

### Page loading

Use `RouteLoading` for page navigation and initial page data. It centers the shared Spinner with the accessible name “Loading page” and visible label “Loading…”. The first application load fills the viewport.

Inside the dashboard it fills the content area while the header and sidebar remain visible. Do not add route-specific skeleton layouts. Buttons, table refreshes, and other local actions keep their own loading cues.

### Tables and record dates

DataTable uses 13px sentence-case headings and 14px sans body cells. Mark the identity column with `primary` to strengthen its hierarchy.
Shared cell tokens provide 16px horizontal and 12px vertical padding. Pages must not override row density individually.
The table palette defines its header, alternating surfaces, dividers, and mint hover or focus treatment in `tokens.css`.
Clickable rows expose an Open column and chevron. Nested buttons, links, and inputs retain their own pointer and keyboard actions.
Stable row keys preserve focus and unfinished edits through refreshes.

Use DataTable's `plain` surface for an approved borderless table. It retains column headings and pagination when empty.

Use its controlled `rowDisclosure` for inline details. The caller owns the selected row, and the toggle exposes its expanded state.
Only the selected row renders detail content.

Keep navigation and disclosure behavior in the shared component rather than recreating it in a page.

Date formatting belongs in `ui/packages/design-system/src/design-system/time-utils.ts` for both the app and shared components.
Render visible timestamps through `Time`; use the exported helpers for plain-text labels and server-side formatting.
Reporting periods use `formatTimeRangeUtc`, and their activity labels use `formatTimeCompactUtc` so both share Coordinated Universal Time (UTC).

The helpers return the shared invalid-date fallback for malformed inputs; ranges also reject an end before the start.
Extend these shared helpers when a reusable format is missing. Do not add a page-local formatter.

Label creation timestamps Created across tables and detail views. Other timestamps name their event: Updated, Last used, Occurred, Expires, or Joined / invited. Use the shared app timestamp labels; avoid a generic Time or Date heading.
Record lists place the timestamp after the record identity and before Actions.
Secrets and API Keys fold creation metadata beneath Name on narrow screens, keeping Actions visible.
The separate Created column returns at the small-screen breakpoint.
Event and runner activity feeds keep their primary timestamp before metrics and status columns; the Runs repeat count closes the event row.
Narrow activity tables retain Status and Inspect. Secondary actor and metric columns may hide before the outcome does.
A feed row never explains a failure. Status flags it and the row's own Inspect dialog carries the reason, its recorded cause and the fix hint; a column repeating a shortened copy of that would be empty on every healthy row.
Row actions run Edit first, then view or publish, then fetch or switch, then Delete, in every table that carries them.
Use IconAction for icon-only row controls so their square size, tooltip, and accessible name stay consistent.
Every row action names its variant: ghost for the neutral ones, destructive for Delete and Deny. IconAction's own default is outline, which draws a box per icon and belongs to standalone buttons, never to a cluster in a table row.
Header controls such as Refresh use a square TooltipButton with `size="sm"`, preserving a minimum 32px target.
Keep loading and completion cues inside that fixed square.
Do not add creation dates to catalog tables whose records have no useful creation-date field.

Column labels and cells share horizontal padding and alignment.
Left-align dates beneath their heading, including secondary usage details in the same cell.
Right-align numeric columns and row actions. Every sort icon follows its label, numeric columns included.
Use shared DataTable sorting, pagination, and scroll behavior.
Default page sizes are 25, 50, and 100. Previous and Next replace the current page; paginated tables never append rows.
Keep the header and pagination footer visible around the scrolling body. Loading disables sorting and paging until the current request settles.
Use Search for text matching. Label structured filters by their fields and state exact-match behavior beside the controls.

Runner detail bounds its table height on desktop and scrolls the page on mobile.
Wrapped headers must never shrink the table viewport until all rows disappear.
Align the table and status line to the same edges. Use the shared extra-large spacing token between working sections.
Status lines use 14px sans-serif text, tabular digits, and keyboard-accessible horizontal scrolling.

Runner cards show host identity, status, last seen, and an Open action. Create runner belongs in the page header.
Runner detail places actions beside the title and status beneath it. Leases and Activity tabs sit directly above their content.
A checks badge reports the result; a separate View checks button makes inspection discoverable.
Activity and Approvals belong in workspace navigation. Keep Approvals available when its inbox is empty. Hide Platform navigation when the person has no operator destinations.

### Forms and popups

Inputs have a visible resting boundary, a mint hover boundary, and a two-pixel focus ring.
Pointer hover must preserve the focus ring on an already focused input.

Chat composers use the shared Textarea's `composer` appearance, with a neutral outer boundary and no inner focus ring.
Focus the composer on entry and restore typing after Send. Conversation updates must preserve focus on other controls.

Dialogs use the shared panel radius, strong border, overlay, and dialog elevation tokens.
Select and dropdown surfaces use the floating elevation token. Motion follows shared timing and respects reduced-motion preferences.

The close control uses Button and reserves space beside the dialog title.
Constrain the panel to the viewport and scroll its content so footer actions remain reachable.
Shared buttons provide a minimum 44-pixel touch target on coarse pointers.

Event details focus on the outcome, request context, and actionable failure guidance. Omit clipboard controls and empty action footers.

Keep mutation errors inside their confirmation dialog, with the failed action available for retry. Prevent dismissal while a destructive request is pending.

Read failures use UnavailableState, which composes the warning Alert with a title, explanation, and recovery actions.
Never substitute an empty result for an unsuccessful read. Reserve destructive alerts for failed changes or destructive decisions.
Workspace and Platform library reads share recovery wording; a missing selected recipe has its own explanation.

Clerk sign-in, sign-up, account menus, and profile forms use `lib/clerkAppearance.ts`.
This adapter maps shared tokens through Clerk’s typed Core 3 appearance API, including `theme`, `options`, and `variables`.
Do not create separate authentication colors or font families.
Clerk’s primary actions, input states, popup backdrop, and corner radii follow the same component roles.
Generated avatar images use the local flat fallback; uploaded user photographs remain visible.
Inspect third-party pseudo-elements for gradients after dependency upgrades.

### Chat composers

Fleet chat and setup chat compose the same controls from `components/domain/chat/ChatComposerFrame.tsx` in the app package.
`ChatComposerFrame` owns the input, boundary, spacing, height limits, and focus behavior.

`ChatComposerModel` owns model display and selection. `ChatComposerHint` owns supporting copy below the input.

Callers supply content and actions. Callers must not override these controls' typography or copy their visual classes.

| Part | Typography | Shared control |
|---|---|---|
| Input | Sans, conversation size and line height | Textarea with `composer` appearance |
| Model | Sans, eyebrow size, label line height | ChatComposerModel; SelectTrigger with `composer` appearance when editable |
| Hint | Sans, eyebrow size, label line height | ChatComposerHint |
| Send or stop | Shared icon button sizing and focus | Button |

Fleet chat displays the saved runtime model. Setup chat selects the model that helps write the setup.

These controls have different jobs and share typography. Changing the setup conversation's model does not change the fleet's runtime model.

On the authoring page for a saved fleet, the header offers Save changes.
Reuse the reviewed draft's save action, including validation, pending state, and conflict handling. Do not add a second save action above the composer.

Explain when changes affect the fleet in plain language. Keep revision numbers within setup details.

Completed replies keep the same markdown renderer and text nodes used while streaming.
Reserve reply-action space before completion. Completing a live reply must not enable layout skipping or change its scroll position.

Only replies loaded as completed history may skip offscreen layout.
Run details use a compact, optional popover with keyboard dismissal and focus return.
Show measured durations with millisecond precision, such as `842ms` or `1.237s`.

Browser checks compare computed font family, size, weight, and line height across both composers.
Check narrow screens, keyboard focus, reduced motion, and repeated reply completion. Completion must not introduce visible movement.

### Ownership and enforcement

Change font families and light/dark color values in `ui/packages/design-system/src/tokens.css`.
Keep token values and theme overrides in `tokens.css`. Keep shared selectors, animations, and reduced-motion behavior in `interactions.css`.

Forward token roles to utilities in `theme.css`; references to token variables are intentional and must not copy their values.
Remove unused roles and duplicate selectors together with their forwards and class-merging entries.

Shared components own their typography,
surface, focus, disabled, and selected styles. A component must render correctly without
consumer CSS; `UsageBar` owns its fill, and `NavItem` owns destination styling.

Pages compose layout and data. Use component variants for visual differences.
Do not copy a component's visual class string into a page or override its font or color
to create a local theme. Add a shared variant when an actual product state requires one.
Interface eyebrows use sans. Apply mono to the technical value itself, never its surrounding
navigation, explanatory prose, or whole table. Keep route labels and recovery actions sans.

Use `text-foreground`, `text-muted-foreground`, and `warning` utilities for new interface code.
Keep distinct roles such as card, popover, focus, and destructive even when their current values match.
See the [CSS audit](reviews/design-system-css-audit.md) for existing duplicate names and compiler evidence.

Named utilities follow TS-TOKEN and TS-TOKENADD in [the TypeScript standard](../.orly/standards/languages/typescript.md), which the `orly-standards-reviewer` agent applies.
This implementation uses layered token surfaces and light.
Component tests verify defaults and state behavior.
No app page or component overrides the font of a shared interface primitive, whether
through an aliased import or a named class.
Browser review checks both themes,
narrow screens, and loading, empty, error, selected, and disabled states. Static checks
cannot determine whether arbitrary prose is a technical value; that remains a review duty.

## Motion

Make movement explain arrival, focus, selection, and disclosure. A finished interface should feel fluid when the person moves through it.

| Interaction | Shared timing | Treatment |
|---|---|---|
| Hover and press | Snap, 120 ms | Color, boundary and a small press response |
| Page arrival | Fade, 240 ms | Opacity only; route loaders and resolved pages keep their position |
| Fleet-section arrival | Fade, 240 ms | Opacity and a small upward settle |
| Chat entrance and interactive cards | Fade, 240 ms | Gentle settle and card lift; chat focus stays quiet |
| Disclosure | Shared Accordion timing | Height follows the revealed content |

Use shared `PageLayout` for page arrivals and the shared entrance class for fleet sections.
Animate opacity and transforms for entrance effects. End without a retained transform so fixed dialogs keep their viewport positioning.
Avoid JavaScript animation loops, animated blur, and continuous decorative movement across large surfaces.
Keep conversation rows and controls stable while typing. Hover must not shift neighboring content or resize a target.
The live indicator may pulse only when the underlying entity is live.
Reduced motion leaves a static, readable state without dimming the content.
Terminal demonstrations may reveal lines when their final content remains available without animation.

## Website product entrance

The website leads with an incident-investigation request composer and one inspectable example conversation.
Explain a fleet once, after the job is clear. Keep docs, source, and sign-in accessible.
Use the shared charcoal, warm off-white, and mint identity.
`ui/packages/design-system/src/tokens.css` owns the palette across website and app.

The user authorized website motion and gradients on 2026-10-13, then extended the design freedom across the app on 2026-10-18.
An original, low-contrast dithered signal field may use an SVG gradient mask and a
brief entrance animation behind the hero. It cannot intercept input or carry essential information.
Foreground surfaces stay readable. Use the shared palette and purposeful depth when choosing decorative treatments.

The conversation types a problem, refines setup, shows a reviewed save, and continues into scheduled work.
Save creates an independent fleet from the reviewed setup. It does not publish to the Library.
Save to Library is a separate, explicit action that publishes an immutable version.
Choose a Library version before creating a new private setup; show the selected version throughout review.
Improve and Apply revise an existing fleet through a new review rather than creating another fleet.
Use native setup records throughout. Do not offer file imports, bundle exports, `SKILL.md`, or `TRIGGER.md` authoring.

### Workspace authoring and publication

The sidebar's **Fleets** section lists saved, authorized fleets. Each row shows the fleet name, a status dot, and a delete control.

Private drafts appear under **Unfinished setups** on **New fleet**. Each row offers **Resume** and a delete control.

Keep private setup status available to screen readers and in the tooltip. Active fleet dots use the shared mint token.

Chat titles use the same 14px Instrument Sans as navigation, with ellipsis and the full title on hover or keyboard focus.

Distinguish loading, unavailable history, and a successful empty list. Failed loading must never look like an empty history.

Keep every accepted message and saved assistant reply in the creator-private draft, subject to deletion and the documented retention rules. Do not select a subset of messages to retain based on model judgment.

During conversation, the selected authoring model turns the person's requirements into proposed instructions and configuration through `propose_setup`. The daemon validates each proposal and stores a new immutable candidate revision. Validation proves supported structure and policy; it does not prove that every conversational requirement was captured.

Keep **Instructions** and explicit publication in the authoring chat menu throughout the conversation.
**Instructions** opens a tall side dialog on desktop and a full-screen editor on mobile.

Questions and requested changes continue the conversation. A standalone Save command finalizes the current decisions with the model, then saves the checked setup.
Publication remains a separate menu action and stores the selected candidate unchanged; the transcript stays private.

Saved fleets return to the same authoring page through **Edit setup**. Instructions and publication use the same menu controls.

Opening the editor creates no fleet and requests no run. Keep the composer mounted and retain unsent text while the dialog is open.

The instruction editor offers **Save changes** and **Cancel**. Warn before discarding unsaved edits; a conflicting setup change retains the manual text.

Failed replies retain partial answers and explain the next action in plain language.
Keep diagnostic details behind **View details**. Unknown usage stays unknown; never substitute zero tokens or zero cost.

**Try again** sends the original request. Saved chat offers it only for the signed-in person's own request.

Show **Saving setup...** through chat finalization and saving, then one white **Save done vN** acknowledgement in chronological order. Readiness marks and access actions remain distinct.

Saving changes updates the version from the server response. New runs use the saved setup, while runs already started keep their version. Accepted work can still be running.

**Save fleet** requests no first run. Configured automatic triggers can start work once setup is ready.

Keep unfinished setup and first-run recovery, schedule registration warnings, and retry actions visible. Successful setup does not add an expandable report to the conversation.
The saved result shares the conversation's scroll area so **Open fleet** remains reachable on narrow screens and short windows.

Review dialogs use the shared viewport height limit and scroll within it.
Setup and runtime compact older conversation near 90 percent of the selected model window, or earlier when operating limits or output headroom require it.
Keep the original history.

If compaction fails or pinned input still cannot fit, show the recorded failure and recovery action.
[Conversation compaction](architecture/context_lifecycle.md) owns the shared behavior and limits.

These boundaries follow the [model instructions](../rustd/crates/afd_authoring/src/model/provider/prompt.rs), [publication implementation](../rustd/crates/afd_authoring/src/library/publish/persist.rs), and [resource reference](architecture/api_resources.md).

### Example playback

Append messages in one bounded native scroll region. The branded signal loader names its current activity.
A blinking cursor follows the typing text. Use Bricolage Grotesque for the entrance headline,
Instrument Sans for messages, and CommitMono for commands and technical excerpts.

Loop the example after a readable hold on the final report. Remove stage tabs and Replay.
Keep Pause and Read full conversation available; the primary example action is Create fleet.
The conversation scroll region must be reachable by keyboard and show the shared focus indicator.
Reading the full conversation transfers focus to the stable Play control when its reveal action disappears.
Pause timers offscreen, in hidden tabs, and when a person inspects evidence.
Manual scrolling stops following while playback continues; show Jump to latest without moving the document.
Visibility recovery preserves the reading position. Explicit Play resets inspection and following.
Evidence inspection stays paused until Play; closing restores the reading position and citation focus.
Reduced motion shows the complete conversation immediately. Mobile keeps the same bounded conversation.
Animation never grants permission, connects tools, authenticates, or starts real work.

Keep Example run visible and disclose that authoring illustrates the planned experience.
Every identifier, excerpt, and output is sample data; arbitrary visitor input never changes the sample diagnosis.
Separate investigation, recommendation, access limits, and report. Repair remains a separately configured job.
Do not show child-role progress until the runtime supplies real child lifecycle and origin facts.

The anonymous entrance has one composer, three useful job suggestions, and an optional CLI entry.
Investigate an incident, Catch test regressions, and Automate recurring work each open their own animated example directly.
Remove Tools & setup and its preview dialog. Opening signup in a new tab preserves page text.
Written requests stay in page memory; never forward them through URLs, analytics, or browser storage.
CLI setup names Claude Code, Codex, and OpenCode and uses the shared Terminal copy control.
Keep installer release status in installation documentation; the entrance has no preview or planned label.
A future authenticated handoff needs its reviewed draft APIs before signup can retain a server-saved request.

The website JavaScript budget is 128 kB gzip; the CSS budget remains 20 kB.

## Product copy and pricing

Explain what a fleet does, what it reads, and what the user can inspect.
Keep backend implementation details out of product instructions.
Retain explicit approval requirements wherever an action requires approval.

The website invites users to create their first fleet and provide workflow feedback.
Indy approved “$0 to start · $25 credit included” on 2026-10-14, replacing the earlier early-access copy.
Show this offer near the welcome heading and in the pricing card. Describe credit as running the first jobs; keep model terminology out of this offer.
Explain that using a personal key bills usage directly with the provider.
Do not publish unapproved future prices, savings estimates, unlimited free usage, or behavior after the included credit runs out.
The authenticated app still displays actual balances and charges from its existing billing data.
Do not add subscriptions, payment methods, or automatic reload controls without supporting behavior.
Enterprise discussions must distinguish offered capabilities from requested arrangements.

Address both audiences without separate homepages. Founders need concrete jobs and visible results; infrastructure leads need access boundaries, evidence, and spending controls.
Show setup requirements after the product example. Do not promise instant setup or automatic access from a waitlist submission.

## Interaction principles

Expressed intent proceeds without redundant confirmation.
Credentials and prerequisites appear at the point of need when supported.
Loading, success, and failure states remain visible where the action began.
Reuse the signed-in session and server credential helper; preserve current access checks on every private read, write, and stream reconnect.
Use one private Server-Sent Events (SSE) connection for active authoring. Coalesce progress hints into durable-state refreshes and reconcile on reconnect. A healthy quiet stream must not become a polling loop. Abort streams and reads when the account, workspace, creator, or draft changes.
Keep private conversation responses uncached. Cache public assets and immutable reusable content only within the appropriate authorization scope; live readiness, permissions, and mutable Library heads require current reads.
If navigation leaves an empty route slot, the shell shows a spinner until route content returns.
Destructive actions retain their existing confirmation behavior.

## Implementation and verification

1. Define values in `ui/packages/design-system/src/tokens.css`.
2. Forward named utilities in `ui/packages/design-system/src/theme.css`.
3. Apply roles inside shared primitives.
4. Migrate explicit consumer styles.
5. Inspect app and website in dark, light, narrow, keyboard, and reduced-motion states.

Use the development-only `/_design-system` gallery as the visual reference. Production builds omit its route and gallery bundle.
Keep token mappings, accessible contrast, and meaningful component behavior covered.
Complete the owned implementation before running focused behavioral checks.
Run full repository unit and integration suites immediately before the PR, as directed by the user.
Record browser evidence for every reachable route family, including empty, error, populated, and selected states where available.
Check narrow-screen overflow, keyboard access, reduced motion, layout shifts, and interaction delays before approving visual changes.
Remove unused styles after checking source references. Shared components own reusable styles; app styles cover app-specific composition.

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
| 2026-06-23 | Retire the pill tab; one underline tab style (app) | M98 §1.1. The app shipped two tab visuals (a `bg-muted` pill tray with a `bg-background` active fill) applied inconsistently. Unified to a single underline: active = a `--pulse` 2px bottom-border on a `--border` rail; inactive `--text-muted`. Shared `design-system/tab-styles.ts` consumed by `Tabs` (Radix) + `TabNav` (links) + their tests (RULE UFS). Approved in the M98 mockup (removed Sep 18, 2026; the file is in `2c1040aef`). |
| 2026-06-23 | Add a scoped motion pass to the operator dashboard (mount-rise, ambient glow-drift, hover/press micro-interactions) | M98 §1.5. A lived-in operator dashboard performs where marketing stays still — approved by Indy as full mockup motion (the M98 mockup, signed off screen-by-screen; removed Sep 18, 2026, the file is in `2c1040aef`). Supersedes the 2026-05-08 "everything else functional or absent" call and the §Motion "page transitions instant / operational software does not perform" rule **for the gated app dashboard only**; marketing + docs keep the restraint. Every effect is reduced-motion-gated; measured by `app/tests/e2e/acceptance/reduced-motion.spec.ts`. |
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
| 2026-09-13 | 13px carries a sans role as well as a mono one | An audit measured every `DataTable` cell at 13px sans (`DataTableView.tsx:279` sets `font-sans text-mono`) while the scale gave 13px only to mono, making the smallest documented sans step 14px. Code and doc disagreed; the code was right. Table density was a deliberate choice — `Pagination.tsx` reasons about it — so the step is documented rather than removed, and no rendered size changed. |
| 2026-09-13 | Larger labels, clearer status colors, and mobile table access | Labels use 13px; operational status uses 14px sans. Contrast checks cover both themes. Mobile layouts preserve the wordmark, actions, and runner rows. |
| 2026-10-13 | Website job entrance with controlled product motion | User explicitly authorized overriding still illustrations and no gradients for the website. An original dithered SVG mask and brief scenes explain work. The preview cannot activate a fleet; a future hosted handoff requires review and explicit execution. |
| 2026-10-13 | Shared charcoal, warm-white, and mint identity | User approved applying the website direction to the shared design system and app. One token palette and a static signal mark serve both surfaces. Actual live indicators keep their existing meaning. |
| 2026-10-15 | Native setup, explicit Library publication, and accessible conversation playback | Save creates a fleet; publishing an immutable Library version is separate. File-based authoring is removed. Website and app retain the approved shared identity, with keyboard focus and reading position preserved during conversation inspection. |
| 2026-10-15 | Compact controls, stronger typography, and clearer color | User authorized a modern, bold, simple, more colorful system with smaller components. Shared controls use a 36px desktop rhythm, semibold labels, mint-tinted secondary actions, and clearer semantic badge borders; coarse-pointer targets remain 44px. |

| 2026-10-15 | Shared table hierarchy and Runner navigation | Live visual approval covers sentence-case table headings, readable rows, shared pagination, dialogs, Runner cards and controls, and workspace Activity placement. Table colors, spacing, and elevations remain shared tokens. |
| 2026-10-18 | Fluid, visually rich app surfaces within the approved identity | Indy authorized modernizing this guidance, deliberate gradients and motion, and an app-wide browser review. Branding, font families, and the PR #752 / M227 palette stay unchanged. Shared components own visual behavior; app widgets own product composition. |
