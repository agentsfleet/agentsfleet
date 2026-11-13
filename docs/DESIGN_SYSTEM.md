# agentsfleet design system

**Direction:** Clear Signal · **Updated:** 2026-11-12

This document governs the app, website, and shared component library.
Clear Signal names the visual direction; agentsfleet remains the product name.
The website gallery demonstrates the components.

Indy approved the refreshed app layout and Manrope scale on November 11, 2026, with compact icons required.
The current code uses the refreshed tokens and components. Deployment and final acceptance remain pending.

The app approval does not change website display sizing or approve new public API behavior.

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
The October direction established the branding in Pull Request (PR) #752 and M227.

The November app approval replaces its interface font, type scale, navigation, and panel treatment with the rules below.

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
| Display | Manrope | Website hero and large section headings |
| Interface and reading | Manrope | Website and app titles, navigation, controls, tables, and conversation |
| Technical | Commit Mono | Code, logs, identifiers, exact timestamps, and technical values |

Bundle fonts locally and load them through the shared theme. Keep `font-display`, `font-sans`, and `font-mono` as shared roles.
Manrope is shared across the website and app, as explicitly requested on November 11, 2026.

Both display and interface roles resolve to it. Commit Mono remains reserved for technical content.
Remove consumer font overrides during migration, including Usage's local system-font treatment.

Keep website display sizes separate from the app scale.

### Approved app scale

Sizes below assume a 16px browser root. Define rem equivalents in shared tokens and preserve browser zoom.
Indy's inspected Amp references establish the compact scale; Manrope remains the approved family.

| Role | Desktop size / line height | Weight | Phone |
|---|---|---|---|
| Conversation and composer | 13px / 20px | 400 | Reading 16px / 26px; typing 16px / 24px |
| Navigation, controls, field labels | 13px / 20px | 500 | Same; typing fields 16px / 24px |
| Summary decisions, output names, descriptions | 13px / 20px | 400; emphasis 500 | Same |
| Table cells and supporting descriptions | 13px / 20px | 400; identity 500 | Same |
| Metadata, timestamps, counts | 12px / 16px | 400 | Same |
| Fleet header and section labels | 13px / 20px | 600 | Same |
| App page title | 20px / 26px | 650 | Same |
| Conversation heading | 19px / 24.7px | 650 | Same |
| Model explanation and prompt strip | 14px / 20px | 400 | Same |
| Dialog title | 18px / 24px | 650 | Same |
| Technical content | 13px / 20px | 400 | Same, with bounded scrolling |

Use zero added letter spacing for interface text. Use sentence case for navigation, actions, and table headings.
Keep ordinary reading at weight 400; use stronger weight for headings and emphasis.

The earlier 14px, 15px, and 16px desktop chat proposals are superseded.

Keep essential decisions and error recovery above the metadata scale. Use 12px for supporting facts, never an entire approval description.
Use tabular numerals for changing values and numeric columns.

Reserve monospace for technical values whose character alignment helps comparison.

Use a shared phone breakpoint for the 16px reading scale. Coarse-pointer typing fields retain 16px even in wider layouts.
Verify computed family, size, weight, and line height after the font loads.

Compare both themes at matching viewport sizes and 100% zoom.
Check 200% zoom separately for wrapping, reachable actions, and retained focus.

### Website display scale

Retain the website's 72px maximum hero, 40px section headings, 18px introduction, and 15px body roles.
Carry the shared interface family through website controls when shared tokens migrate. The app approval does not authorize a new website layout.

## Icons

Use Phosphor for general interface icons. Remove Lucide imports as the corresponding shared components migrate; the finished app uses one family.
Keep the product brand mark separate from the icon library.

| Role | Glyph size | Interactive target |
|---|---|---|
| Main rail, including New fleet | 20px | 48px desktop; at least 44px on phones |
| Inline actions, menus, table controls, status | 16px | 32–36px fine pointer; at least 44px coarse pointer |
| Composer attach and send controls | 18px | 36px fine pointer; at least 44px coarse pointer |
| Small chevrons | 12–14px | The whole labeled row is interactive |
| Rail brand mark | 24px | At least 44px |

Use regular weight by default. Selected navigation may use Phosphor’s duotone weight without changing glyph dimensions or moving surrounding content.
New fleet uses Phosphor `note-pencil`, the square writing pad with a pencil.

Plain pencils identify editing an existing item.
Success uses `check-circle`; selection ticks keep their separate meaning.

Icon-only buttons require accessible names and tooltips available on hover and keyboard focus.
Hide decorative icons from assistive technology. Keep status text beside its icon, and keep disclosure chevrons visible before hover.

Do not enlarge glyphs to fill their click targets.

## Color

Dark is the primary brand presentation. Use charcoal surfaces, warm off-white text, and restrained mint. Light has equal usability requirements.

The token file owns shipped values. Migrate the app to the approved values below through shared roles, without page-local colors.

| App role | Dark target | Light target |
|---|---|---|
| Canvas | `#101114` | `#E9EBEE` |
| Working surface | `#181A1E` | `#FAFBFC` |
| Panel / input | `#202328` | `#F0F2F5` |
| Raised / selected surface | `#2B2E35` | `#E3E7EC` |
| Primary text | `#F5F6F8` | `#20242B` |
| Secondary text | `#C2C7D0` | `#495361` |
| Metadata | `#A3ABB7` | `#5E6876` |
| Control border | `#3D424B` | `#BDC5CE` |
| Quiet divider | `#30343C` | `#D7DCE2` |
| Primary action / text | `#C1F0D6` / `#153D29` | `#24593D` / `#FFFFFF` |
| Success | `#B6E4C8` | `#286644` |
| Warning | `#F3CB87` | `#765017` |
| Error / background | `#FFB3B6` / `#302126` | `#A42938` / `#FFF0F1` |

Use semantic foreground tokens on their intended backgrounds.
Check primary, muted, subtle, mint, and status text against every surface where they appear.
Normal text must reach 4.5:1 contrast; focus and control boundaries must remain identifiable.
Never reduce a readable text token through opacity to create secondary copy.

Test both themes with axe-core in the rendered browser and the shared token contrast tests.
Include filled live, warning, error and evidence badges; a foreground passing on the page can still fail inside a badge.
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

Label chips use unfilled surfaces, a line border and 12px/16px regular text. Keep neutral metadata quiet. Reserve filled badges for live, warning, error and evidence signals with explicit semantic text or icons.

App secondary actions use a neutral panel and visible border.

Selected navigation and primary actions carry the mint emphasis.

The shared `BrandMark` is a static eight-ray signal with a central dot, paired with
the wordmark. It identifies the product without implying live execution. Use
`WakePulse` only for actual live state. Shared brand assets live in
`ui/packages/design-system/assets/`; website and app icons reference that source.

## Surfaces and layout

- Keep reading surfaces solid. Decorative light may frame a hero or working surface without reducing text contrast.
- Chat composers show focus through the typing caret and a stronger neutral outer border. Avoid a second outline or mint glow.
- Use borders, surface contrast, and shared elevation roles to establish depth. Floating surfaces remain clearly above the page.
- Use 10px radii for app fields and small controls, 16px for cards, and 22px for the main surface, composer, and Summary.
- Keep the website's existing radius roles until its components migrate.
- Use circles for avatars, status dots, and the compact Send action.
- Use the shared 4px spacing scale.
- Keep related controls close and separate different tasks with clear section gaps.
- Avoid cards nested inside cards when a divider communicates the same grouping.

App layouts use consistent navigation, page titles, section headings, and action positions.
PageLayout owns section spacing. PageHeader owns the title and description.

PageHeader separates the route introduction from its working area with a fine divider and readable supporting copy.
SectionHeader places an action beside the working area it affects.
Descriptions sit below titles and wrap naturally.

Page headers and their actions wrap on narrow screens.
The collapsed app rail uses the brand mark. Its expanded form may show the wordmark.

Keep workspace names available in the workspace picker and preserve at least 44px touch targets.

Website layouts use an editorial grid within the shared content width.
Pair a concise explanation with a concrete product example.
Vary section composition according to its content.
Keep narrow-screen reading order meaningful without relying on visual placement.

### Fleet shell and Summary

Start with a collapsed rail: 72px desktop and 56px on phones. Keep New fleet visible through the square-and-pencil action.
The empty conversation also offers New fleet and a composer.

Do not require opening a menu to begin.

Keep the fleet title, current state, and Fleet actions together in the top bar. Place cost and New fleet at the opposite edge.
Put Rename, scheduling, Archive, and Permanently Delete in the overflow menu.

Archive permanently stops the fleet and retains setup and history. Indy explicitly chose no Restore; use Stop and Resume for temporary interruption.
Offer Permanently Delete directly on active or archived fleets, with its own confirmation.

The app handles any required terminal stop internally.
If stopping succeeds but deletion fails, retain the fleet and offer Retry deletion. Preserve the existing backend authorization and version checks.

After confirmed success, show a compact notification near the top center of the page.
Use **Fleet archived** with **View history** and a dismiss control, or **Fleet permanently deleted** with a dismiss control.
Archive has no Undo or Restore.

Deleted fleets have no View action.
Use 13px / 20px text, a 16px icon, a 10px rounded border, and 44px touch targets on phones.
Announce the outcome and fleet name through a polite live region without moving focus into the notification.

Dismiss success after eight seconds; pause dismissal while hovered or focused. Keep history reachable from the Archived list after dismissal.

Remove the fleet from the active list after success. If it was open, show the New fleet screen and focus its main content.
Keep another open fleet in place when acting on a different fleet from the list.

Archived history is read-only. Pending operations, failures, and uncertain outcomes remain visible beside the action until resolved.
Never show a success notification while a request is pending or when only the stop portion of deletion succeeded.


Summary occupies a 248px column on wide desktops. Use 228px at 801–1100px and 216px at 681–800px; stack at 680px and below. Use a 16px inner inset, 12px at the narrow intermediate step, and 12/16/32px conversation gutters at 681/801/1101px.

Phone shell edges are 4px and Summary outer insets 12px; transcript insets stay 16px.

Align its top with the conversation's working area.

Summary remains inline and visible without clicking. Show the current decision, relevant changes, outputs, and next scheduled wake-up. Keep Changes, Outputs and Sources headings with one quiet line when confirmed empty.

Insert pending approval or blocked work first. Other sections appear only for recorded content. Do not infer an Environment inventory, live background processes or child-agent state from tool calls.

Detailed evidence opens from the relevant item; the Summary itself never becomes a drawer. Keep task objectives, criteria and controls behind a closed Tasks disclosure. Sources shows one repository row or count; its policy belongs inside the same disclosure.

Loading and unavailable reads must not appear as confirmed empty outputs.

Section labels use 12px/16px, with 4px label-to-content spacing, 8px between sections and 8px after each divider. Keep 36px desktop and 44px phone disclosure targets without adding a second spacer row. Conversation input starts at 60px; a 100px empty setup field is a separate composition.

The header edit action opens the existing setup editor. User attachments precede prose in lighter bubbles; calendar boundaries and compact clocks use the same UTC frame and retain exact accessible instants.

On phones, place Summary before the transcript inside the native assistant-ui viewport. Keep its complete content in the page flow, with no height cap or nested scroll area. The native viewport footer stays in the scroll flow and sticks to the bottom while reading.

Jump to latest stays reachable. On desktop, Summary spans the conversation and footer tracks so attachment and error states cannot clip its heading. Mount one Summary reader across breakpoints.

Preserve readable decision text and reachable actions; do not solve crowding with smaller type.

Use the 4px spacing scale: 8px within related controls, 12–16px within groups, and 24–32px between working sections.
Conversation, output cards, and composer share horizontal edges. Align labels and values consistently within Summary and resource rows.

### Conversation outcomes and errors

Keep replies, tool activity, incoming messages, and wake-ups in one chronological conversation. Show the recorded outcome for each activity.
The compact strip above the composer summarizes current changes or a scheduled wake-up; it does not replace the conversation's history.

| Outcome | Visible summary | Inspectable detail |
|---|---|---|
| Command | Action, running or final status, recorded duration | Command, output, exit information |
| File edit | File name and recorded change counts | Diff and resulting file, when supplied |
| Incoming Slack message | Source and message-received state | Recorded message and available source link |
| Outgoing notification | Sending, sent, failed, or unconfirmed | Recipient and delivery result when supplied |
| Scheduled wake-up | Next time and schedule state | Time zone, edit, pause, and cancel actions supported by the API |
| Helper activity | Recorded start, progress, and completion | Result or failure; no invented helper lifecycle |
| Approval | Requested action and consequence | Decision controls, saved decision, and supporting evidence |
| Failure | What failed and the next useful action | Diagnostic detail without credential values |

Keep failures visible beside the failed message or action. A disappearing toast or composer strip must not be the only failure record.
Retain partial results and unsent text.

Retry only the failed step when previous writes succeeded.
Keep unconfirmed external writes distinct from confirmed failures; reconnecting must not resend them blindly.

Keep assistant-ui's existing event-backed runtime. Add renderers for recorded message parts before adding another chat framework.
Attachments, mentions, editing, and branching need their own supported request and storage behavior before their controls become available.

The authorized preview-upload exception uses temporary receipts and makes no storage or analysis claim. Render the SDK's `message.submission` while files are being prepared. Submitted files remain visible; removal belongs to the native composer before Send or after a failed upload returns to the draft.

The installed SDK's message attachment client does not support removal during submission. Leaving the conversation cancels the request. A failed upload never becomes an empty assistant reply or a completed receipt.

Display concise progress facts and outcomes; do not present hidden model reasoning as an activity feed.

### Models, API Keys, and Team

Models offers one Create model dialog with Provider, Model, credential choice, and optional Make active.
The credential choice offers Add new key or Use saved key. Filter saved keys by provider and show their names without stored values.

Use Key name for a new credential. Reusing a key must not rotate or replace it.

Keep Base URL for supported custom endpoints. Every model provider requires an API key, matching the runtime's credential rules. Explain saved-key scope beside the picker when it affects selection.

Model registration and active-model selection are account-scoped.

Each registered model references the workspace that stores its credential.
Use the form's explicit workspace for saved-key choices and new-key storage. Switching workspaces never relocates a key or changes a registered model's source reference.

Show completed steps when key storage, model registration, and activation succeed separately. Keep a registered model visible if activation fails.
Retry activation alone after an activation failure.

Storage success does not establish that a provider key works.

Platform operators set the shared default through the existing `agentsfleet platform keys` commands.
Keep that management in the CLI; this refresh adds no platform-default administration screen.
Account Models retains Create model and Make active.

Its choices never grant platform administration.
Preserve model, platform credential, and vault access checks when renaming or regrouping other surfaces.

API Keys combines API keys, signed-in browser sessions and Profile and sign-in settings.
Provider credentials belong under Models and Secrets.

The Profile and sign-in settings action scrolls to an always-mounted named section on API Keys.
The auth facade embeds the supported provider widget; native password, email and verification flows remain provider-owned.

Changing the subject or session clears the widget. Keep its draft intact during ordinary resource refreshes.
Sign out remains in the avatar menu.

The app owns profile and session data shapes; Clerk supplies the current implementation through a dedicated adapter.
Keep Clerk-specific objects and errors out of product components. A component re-export alone does not make the provider replaceable.

Use Last active for activity timestamps and Signed in only for recorded session creation. Mark the current session and preserve real revoke behavior.
Keep an absent activity time unknown.

Require confirmation and recent verification before signing out another device, and reconcile an uncertain response before repeating the request.
Retain edited profile names after a failed save. Photo, email and authentication-factor changes use the provider's verified account controls behind the adapter.

Keep **Avatar → Team**. Current membership gives access to every workspace in the account; do not label this workspace-only access.
Preserve Owner and Member roles, owner protection, pending invitations, and member removal.

The current invite flow accepts one email and creates a recipient-specific invitation.

If email delivery fails after invitation creation, keep the invitation visible with resend and copy-link recovery.
Keep delivery status separate from invitation status. Never show an empty membership list when its read failed.

Call the resource **Plugins**. Flow 1 is approved: each Plugin contains a complete fleet setup with immutable revisions and existing account access.
Use Plugin starts a new private setup; it never attaches capabilities to an existing fleet.

Save as Plugin remains separate from Save fleet.
Defer capability composition and plugin mentions to the revisit note in M239; they are outside this refresh.
Keep existing Secrets.

A separate readable environment-variable feature is outside this delivery.

## Components

| Component | Rules |
|---|---|
| Button | Compact medium-weight sans label; solid primary fill, neutral panel secondary fill with line border, visible focus, stable disabled state |
| Input and textarea | Compact sans fields with line borders; explicit mono only for code or technical data |
| Navigation | Readable sans labels, clear selected state, consistent icon size |
| Tabs | Shared underline style for both route and local tabs |
| Card | Solid surface, fine border, one clear heading; interactive cards lift and reveal a mint boundary on hover or focus |
| Table | Sans names and descriptions; mono identifiers; aligned numeric values |
| Badge | Label chips use regular 12px/16px text, line borders and no fill; live, warning, error and evidence signals may use their semantic fills |
| Dialog | Clear title, focused first action, keyboard dismissal when safe |
| Empty state | Explain the missing item and provide the next action |
| Error state | Explain what failed and retain the recovery action |
| Usage bar | Solid mint fill with visible numeric context |
| Transcript | Distinguish operator input, fleet response, and external activity through structure |

Existing component behavior remains authoritative.
Presentation changes must preserve keyboard interactions, loading state, and accessible names.
Use shared primitives before adding consumer markup with equivalent behavior.

Resource pages use the shared ResourceHeader for the 64px top bar and PageHeader inside a centered 1040px content column. The top bar includes the New fleet action, uses a subtle bottom border, and aligns its icon 24px from the surface edge. Models restore the separate Platform default card, Your models list, Make active action and provider prompt.

Details and edits use the row menu; server continuation uses Load more without adding the former sort toolbar.

Team groups native lists inside shared panels. Its search remains, with names, initials, roles, explicit joined or expiry dates, and one menu per manageable row. Only the authenticated person's row carries You.

Invite delivery status must reflect the backend, including a retained invite whose email was not delivered.

API Keys contains keys, signed-in sessions and native identity-provider controls behind the auth facade.
The avatar menu opens this combined destination. The header action scrolls to Profile and sign-in settings.

Keys use resource rows and one-time reveal; sessions support individual and all-other-session sign-out. Read all session pages before bulk revocation and exclude this device.

Usage restores the period toolbar, shared metric strip, daily-spend section and By fleet table. Charge details remain in a closed native disclosure, opening for an explicitly selected activity. Do not draw daily bars from a partial activities page: the current report has no daily buckets.

Preserve that section with its unavailable state until one authoritative aggregate is available.

Secrets use Name, Type, hidden Value, Available to and Manage. The current API does not return per-secret fleet access. State that absence; never derive effective authorization from references or show an unbacked access checkbox.

Preserve named-field replacement for credentials that require several values. Plugins and Connected apps use grouped resource rows and shared dialogs. A plugin's actual version appears after inspection; list cards expose only its revision identifier.

Compact phone tables keep action targets inside their visible scrolling surface. Session and key rows let identity text occupy the full width before the action. Server pagination and error recovery remain available where the API requires them.

Reusable visual treatments belong in `ui/packages/design-system`. App widgets combine those primitives with product behavior and data.

Use Activity for the workspace feed. Fleet activity appears in its conversation. Individual records remain events; existing event URLs retain their meaning.

The fleet header offers **Edit setup** and **Fleet actions**. **Edit setup** opens the authoring page at the fleet's saved version.

Fleet messages, setup receipts, and integration activity share one chronological conversation.

**Save fleet** sits beside the authoring chat title. Authoring a saved fleet shows **Save changes** in that position.
**Stop**, **Archive**, and **Permanently Delete** live in **Fleet actions**.

Archive and deletion use separate confirmations.

Setup and saved chat share an aligned conversation and composer. Bound desktop content to the reviewed 740px maximum and keep the composer at the bottom.
Review prose line length at 13px; narrow text blocks when lines become difficult to follow, while allowing commands and diffs more width.

User messages use a quiet bubble; assistant replies use plain prose and the fleet's name.
**Saved memories** opens from **Fleet actions**. The dialog has one title; its description fits on one line on desktop and wraps on narrow screens.

Each tool call has a separate entry with its recorded name, status, and elapsed clock.
A running call has its own spinner. **Input** and **Output** or **Error** open independently for each call.
Commands and returned text stay inside these disclosures. Tool status does not claim the fleet's overall task succeeded.

Create and Edit secret use the same field editor with masked values and add/remove controls.
Both dialogs share the encryption explanation and help link. Edit locks the name and explains that saving replaces every field.
Stored values are never loaded into the editor. Secret names display Copy only; value editing opens from Actions.

### Compact density

Default and small text buttons have a 40px minimum height on fine pointers; large actions are 44px. Form inputs and select triggers are at least 44px, with 40px select triggers in the Usage toolbar. Icon buttons are 40px, compact icon actions are 32px, and composer icon actions are 36px.

Coarse pointers retain a minimum 44px interactive target. Visible density must not make touch, keyboard focus, or long labels harder to use.
Use the approved app type roles above.

Smaller glyphs retain their click targets, and compact controls retain readable labels.
Place related controls in one wrapping row where practical. Keep dialogs focused on the current decision and disclose advanced configuration through the shared Accordion.

Use shared Button, Input, Select, Label, Badge, and StatusPill variants for these decisions. Pages must not recreate their appearance with local class overrides.
Use `Card interactive` only when the surface opens a destination.

Static information panels keep their resting treatment.
Primary controls press briefly; interactive cards lift by one small spacing step and move their Open cue toward the destination.
These effects use shared styles in `interactions.css`.

Reduced motion retains color and focus feedback without movement.

### Page loading

Use `RouteLoading` for page navigation and initial page data. It centers the shared Spinner with the accessible name “Loading page” and visible label “Loading…”. The first application load fills the viewport.

Inside the dashboard it fills the content area while the header and sidebar remain visible. Do not add route-specific skeleton layouts. Buttons, table refreshes, and other local actions keep their own loading cues.

### Tables and record dates

App DataTable uses 13px sentence-case headings and 13px sans body cells. Mark the identity column with `primary` to strengthen its hierarchy.
Shared cell tokens provide 16px horizontal and 12px vertical padding.

Pages must not override row density individually.
The table palette defines its header, alternating surfaces, dividers, and mint hover or focus treatment in `tokens.css`.
Clickable rows expose an Open column and chevron.

Nested buttons, links, and inputs retain their own pointer and keyboard actions.
Stable row keys preserve focus and unfinished edits through refreshes.

Use DataTable's `plain` surface for an approved borderless table. It retains column headings and pagination when empty.

Use its controlled `rowDisclosure` for inline details. The caller owns the selected row, and the toggle exposes its expanded state.
Only the selected row renders detail content.

Keep navigation and disclosure behavior in the shared component rather than recreating it in a page.

Date formatting belongs in `ui/packages/design-system/src/design-system/time-utils.ts` for both the app and shared components. Render visible timestamps through `Time`; use the exported helpers for plain-text labels and server-side formatting. Reporting periods use `formatTimeRangeUtc`, and their activity labels use `formatTimeCompactUtc` so both share Coordinated Universal Time (UTC).

Use `Time format="date"` for joined, expiry and creation dates and `Time format="month"` for a selected reporting month. These use UTC and keep the canonical instant.

The helpers return the shared invalid-date fallback for malformed inputs; ranges also reject an end before the start.
Extend these shared helpers when a reusable format is missing. Do not add a page-local formatter.

Label creation timestamps Created across tables and detail views. Other timestamps name their event: Updated, Last used, Occurred, Expires, or Joined / invited. Use the shared app timestamp labels; avoid a generic Time or Date heading.

Record lists place the timestamp after the record identity and before Actions.

Secrets and API Keys fold creation metadata beneath Name on narrow screens, keeping Actions visible.

The separate Created column returns at the small-screen breakpoint.

Event and runner activity feeds keep their primary timestamp before metrics and status columns; the Runs repeat count closes the event row.

Narrow activity tables retain Status and Inspect. Secondary actor and metric columns may hide before the outcome does.

A feed row flags a failure through Status. Its Inspect dialog carries the reason, recorded cause and fix hint. A separate failure column would be empty on healthy rows.

Row actions run Edit first, then view or publish, then fetch or switch, then Delete, in every table that carries them.

Use IconAction for icon-only row controls so their square size, tooltip, and accessible name stay consistent.

Use Phosphor `check-circle` for confirmed actions, including approval, invitation, setup, refresh, and copy feedback.

Keep existing sizes and semantic colors. Checkbox and menu ticks continue to indicate selection.

Keep approval rows visible while a decision saves; replace their controls with `Saving decision…` until the native response and read-back settle.

Show failures beside the retained row and use the recorded decision metadata after a successful read-back.

Label incoming Slack chat events `Slack · Message received`; their `View message` disclosure shows the recorded message text.

Use `View event` and a document icon for other event data. Do not show a braces icon as the explanation for an incoming message.

Every row action names its variant: ghost for neutral actions, destructive for Delete and Deny. IconAction defaults to outline. Use that boxed style for standalone buttons, rather than table-row clusters.

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
Align the table and status line to the same edges.

Use the shared extra-large spacing token between working sections.
App status lines use 13px sans-serif text, tabular digits, and keyboard-accessible horizontal scrolling.

Runner cards show host identity, status, last seen, and an Open action. Create runner belongs in the page header.
Runner detail places actions beside the title and status beneath it.

Leases and Activity tabs sit directly above their content.
A checks badge reports the result; a separate View checks button makes inspection discoverable.
Fleet activity belongs in its conversation.

Pending fleet decisions appear in Summary. Approve and Deny act on the recorded request in the fleet conversation. Show action details inline.

Remove standalone Approvals and Grants destinations. Retain authorization and standing-grant enforcement.

### Forms and popups

Runner configuration uses a full-height right panel with a fixed header and save footer.
Only the field area scrolls; save feedback stays beside the actions.
Execution, network access, durable work, and advanced settings share the same field composition in enrollment and editing.
Network choices reflect the three supported policies.
Package download sites apply only to approved destinations; custom hosts replace the standard list.
Isolation choices describe assigned enforcement, rather than selectable engines.
The panel uses shared dialog motion and honors Reduce Motion.

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

Existing Clerk forms use `lib/clerkAppearance.ts` until custom account surfaces replace them.
This adapter maps shared tokens through Clerk’s typed Core 3 appearance API, including `theme`, `options`, and `variables`.
Do not create separate authentication colors or font families.

Clerk’s primary actions, input states, popup backdrop, and corner radii follow the same component roles.
Generated avatar images use the local flat fallback; uploaded user photographs remain visible.
Inspect third-party pseudo-elements for gradients after dependency upgrades.

### Typed conversation components

Keep assistant-ui's existing runtime as the conversation owner. Normalize recorded API events into typed message and tool parts before rendering. Keep the shared row responsible for layout, sender identity and metadata.

Dedicated components render text, command output, file changes, approvals, schedules, sources, outputs and durable failures.

Register presentation by a known part/tool kind. Adding an outcome supplies its component, input/result schema and state handling. It must not add branches to the shared row, run generated markup, infer success from prose or acquire permissions through presentation.

Unknown tools retain a readable generic disclosure with their recorded name, input and outcome.

Use assistant-ui's native GroupedParts pipeline and render-only backend tool registration. Keep a typed adapter for recorded fleet events and application-specific result formatting. Commands, file reads, edits and plans are parts of a reply.

Reuse the official terminal and diff elements; inline code, fenced code, tables and prose links use the shared Markdown renderer and its code-header slot. Tool arguments and returned output remain separate; a diff preview alone never proves an edit succeeded.

A missing specialized view uses readable text or a safe resource name/link. Keep recorded activity text and outcomes when grouped details are malformed, with a quiet details-unavailable note. Show a preview-unavailable label only when no readable content exists.

Never print arbitrary payload objects or model-provided component definitions. Activity links use the shared Button link variant with 16px glyphs and 36px desktop / 44px phone targets.

Preview uploads use native composer/message attachment primitives and an authenticated acknowledgement stub. The composer allows up to five files, each at most 5 MiB. The temporary endpoint checks the current fleet's access and returns fixed plain text.

It does not read or parse the request body. It does not inspect files, store bytes or run analysis. Temporary preview turns never enter durable fleet history and clear when the account or conversation changes.

Archive remains read-only. Once a backend is selected, the route becomes a forwarding boundary and that backend owns upload validation, storage and ingestion.

Running, succeeded, failed, interrupted and unconfirmed are separate recorded states. A proposed change and an applied change remain distinct. Keep requests bound to their workspace, fleet and operation identity; cancellation or navigation must prevent a late result appearing in another conversation.

Appsmith's [widget registration](https://github.com/appsmithorg/appsmith/blob/65921d7294476c2f7af25c3c7d31c43da2eee215/app/client/src/WidgetProvider/factory/index.tsx) and [AI request-context boundary](https://github.com/appsmithorg/appsmith/blob/65921d7294476c2f7af25c3c7d31c43da2eee215/app/client/src/sagas/AIAssistantRequestContext.ts) inform this composition. Their custom widget editor is not our chat runtime; use assistant-ui message parts and tool components with the existing backend events.

### Shared composer controls

Fleet chat and setup chat compose the same controls from `components/domain/chat/ChatComposerFrame.tsx` in the app package.
`ChatComposerFrame` owns the input, boundary, spacing, height limits, and focus behavior.
Place the model and contextual controls at the left of the footer, with the circular Send action at the right, matching the approved layout.

Keep both chat purposes on the same frame and align the composer with the conversation's reading column.

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

Explain when changes affect the fleet in plain language. Show confirmed saved versions in the compact status above the composer.
Keep full revision and access details in setup inspection.

Use a labeled pencil action for editing an existing setup. Keep Summary visible without a toggle or drawer.
Existing task providers retain request ownership; Summary reads their state without starting duplicate requests.

Keep durable tasks visible and omit empty Outputs and Changes sections.
The saved setup editor returns to its fleet conversation. Use the rail's fleet picker to switch fleets.

### Setup-to-conversation transition

Keep one visual shell through setup, confirmed save, and the fleet conversation. Preserve the title, reading width, composer position, and inline Summary.
Label the authoring state Setup and its action Save fleet.

Show goal, model, access, and trigger requirements beside the conversation.

After the server confirms the fleet identity, open its conversation without an extra Open fleet action. Replace the draft URL with the saved fleet URL.
Show confirmed readiness separately from saving.

If setup needs attention after creation, retain the created fleet and expose recovery without repeating creation.

Use Message fleet after the transition. Edit setup explicitly returns to authoring within the same shell; normal runtime messages never silently rewrite setup.
Save changes returns to the conversation after confirmed revision acceptance.

Preserve version checks and conflict recovery.

Retain unsent text and scroll position, with separate buffers for setup and runtime. Never submit a setup draft to the runtime endpoint during navigation.
Keep the creator's setup transcript private.

Other members see only setup facts and activity already authorized for them.

Saving does not replay the setup prompt as a first run. Run now requires an explicit instruction; configured triggers retain their existing behavior.
Save as Plugin publishes a reusable complete setup through a separate action.

### Chat state and activity

Use assistant-ui's runtime, message parts, streaming renderer and elapsed-call hook.
The native API adapter owns requests, cancellation and retry identities; the Rust API owns saved versions and durable outcomes.
React owns local interactions such as open dialogs and expanded rows.

Keep derived message projections pure and preserve unchanged row identities.
Reuse the existing scoped caches and per-call result cache before adding another state store.
Measure production bundles and representative updates before adopting a new state library.

Assistant replies use the full reading column. User messages and steering requests use narrower, right-aligned bubbles.
Long messages offer Show more and Show less; replies already being read stay open when streaming finishes.

Group tool calls under Ran commands, with individual expandable commands and visible outcome labels.
Load full output when requested. Pair loading and running labels with the shared animated signal indicator.

Use react-window for output above 200 retained lines so long output mounts only the visible rows.
Preserve wrapped lines, original line numbers, keyboard scrolling and literal text.
Provide search across every retained line and copy the complete output, including lines outside the current viewport.

The compact setup status shows actual reading, sending, saving and publishing progress.
Show a success tick only for a confirmed saved version or publication.
A new authoring request replaces the previous confirmation and its fleet action.

Confirmed saving opens the fleet conversation. Publishing a Plugin separately offers New fleet pinned to the published revision.
Flow 1 is approved: Plugins preserve full-fleet reuse.

Attachable capabilities are deferred to the M239 discussion.
Keep those navigation actions separate from the Save changes control.

Completed replies keep the same markdown renderer and text nodes used while streaming.
Reserve reply-action space before completion. Completing a live reply must not enable layout skipping or change its scroll position.

Only replies loaded as completed history may skip offscreen layout.
Run details use a compact, optional popover with keyboard dismissal and focus return.
Show measured durations with millisecond precision, such as `842ms` or `1.237s`.

Browser checks compare computed font family, size, weight, and line height across both composers.
Check narrow screens, keyboard focus, reduced motion, and repeated reply completion. Completion must not introduce visible movement.

### Ownership and enforcement

`ui/packages/design-system/src/tokens.css` owns shared default values, including the website's typography and palette.
`ui/packages/design-system/src/app-tokens.css` owns the approved dashboard overrides: compact typography sizes, app colors, radii, and responsive reading sizes. Font families and imports belong in shared `tokens.css`.

The app imports its overrides after the defaults; the website does not import them.
Keep both files limited to values and font imports. Define every Tailwind utility mapping once, in `theme.css`; app overrides must not redeclare those mappings.

Keep shared selectors, animations, and reduced-motion behavior in `interactions.css`.

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

Fleet setup and Fleet conversation share a moving text highlight for active progress labels.
Use the existing `tool-shimmer` timing and `data-chat-loading` selector in `interactions.css`.
Animate paint without changing text geometry, and remove the loading state when work settles.

Queued text stays static.
Reduced-motion and forced-color preferences show static, readable text.

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
Animate genuine loading and running work by default, using the shared signal indicator beside a clear status label.

Stop the indicator when that work succeeds, fails or stops. Keep the final result visible with its icon and text.

Honor an explicit OS Reduce Motion preference with static icons, readable status text and immediate state changes.
Reduced motion must preserve progress and outcomes without dimming content.
Terminal demonstrations may reveal lines when their final content remains available without animation.

## Website product entrance

The website leads with an incident-investigation request composer and one inspectable example conversation.
Explain a fleet once, after the job is clear. Keep docs, source, and sign-in accessible.

Use the shared charcoal, warm off-white, and mint identity.
`ui/packages/design-system/src/tokens.css` owns the shared palette defaults. The dashboard's approved theme overrides live in `app-tokens.css`; website pages retain the shared defaults.

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

The **Fleets** picker lists saved, authorized fleets with names and status. Put Delete in Fleet actions, away from the main navigation target.

Private drafts appear under **Unfinished setups** on **New fleet**. Each row offers **Resume** and a delete control.

Keep private setup status available to screen readers and in the tooltip. Active fleet dots use the shared mint token.

App chat titles use Manrope 13px / 20px, with ellipsis and the full title available on hover or keyboard focus.
Tool activity uses 16px icons with text for every outcome.

Keep at least a 44px disclosure target on mobile and put long targets below the action.
Desktop activity disclosures have a 36px minimum height and 13px action text; supporting metadata uses 12px / 16px.

Keep 8px inner padding around activity controls; keyboard focus uses one solid inset outline.

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
Show the saved acknowledgement and recovery actions within the conversation on narrow screens and short windows.

Open the confirmed fleet conversation after saving; do not require another navigation action.

Review dialogs use the shared viewport height limit and scroll within it.
Setup and runtime compact older conversation near 90 percent of the selected model window, or earlier when operating limits or output headroom require it.
Keep the original history.

If compaction fails or pinned input still cannot fit, show the recorded failure and recovery action.
[Conversation compaction](architecture/context_lifecycle.md) owns the shared behavior and limits.

These boundaries follow the [model instructions](../rustd/crates/daemon/afd_authoring/src/model/provider/prompt.rs), [publication implementation](../rustd/crates/daemon/afd_authoring/src/library/publish/persist.rs), and [resource reference](architecture/api_resources.md).

### Example playback

Append messages in one bounded native scroll region. The branded signal loader names its current activity.
A blinking cursor follows the typing text.

Use Manrope for the entrance headline,
the shared interface face for messages, and Commit Mono for commands and technical excerpts.

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

1. Define shared default values in `ui/packages/design-system/src/tokens.css`. Put dashboard-only overrides in `app-tokens.css`.
2. Forward named utilities in `ui/packages/design-system/src/theme.css`.
3. Apply roles inside shared primitives.
4. Migrate explicit consumer styles.
5. Inspect app and website in dark, light, narrow, keyboard, and reduced-motion states.

Finish the converted app with a rendered visual review, after real data and fonts load.
Review conversation, activity, errors, wake-ups, empty state, Models, Create model, Plugins, Secrets, Usage, API Keys, Team, and invitations.

| Check | Required observation |
|---|---|
| Viewports | 390px phone, 768px tablet, and 1480px desktop; both themes |
| Type and icons | Computed approved sizes; stable glyph dimensions across selected and default states |
| Spacing | Shared left edges, consistent row insets, balanced section gaps, and aligned dialog actions |
| Summary | Visible inline; phone content does not push the entire conversation below the first screen |
| Long content | Names wrap or expose their full value; commands scroll within their own surface |
| Controls | Visible focus, one search focus ring, reachable footer actions, and correct pointer target sizes |
| States | Loading, empty, failed, partial success, disabled, and selected states retain their structure |
| Zoom and motion | 200% zoom preserves access; reduced motion preserves all state information |

Inspect screenshots directly and exercise the interactions. Automated accessibility checks supplement visual judgment.
Record remaining visual defects and their affected screens.

Prototype checks do not prove the converted app matches the approved design.

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
| 2026-11-01 | Animate ongoing work and keep chat state ownership explicit | Indy confirmed normal animation with the OS Reduce Motion preference respected. Chat uses assistant-ui, native API operations, scoped caches and local interaction state; progress icons reflect actual work. |
| 2026-11-07 | Group runner configuration in a right panel | Execution, network access, durable work, and advanced settings share readable field groups. Editing keeps Save visible while fields scroll. |
| 2026-11-11 | Approved app refresh and compact icons | Manrope 13px / 20px desktop reading, 12px / 16px metadata, 16px phone reading; Phosphor 20px rail and 16px actions. Summary stays inline. Rounded panels, Team, Models, Usage, and Security follow the reviewed layouts. Production conversion and final rendered spacing review remain due. |

### Recoverable chat failures and tool summaries

A recoverable chat failure is a compact, politely announced inline status with a 16px enclosed warning icon and expandable diagnostics.
Keep the submitted request and partial response; reconnecting a browser stream does not authorize replaying an uncertain tool write.
Use recorded action verbs in collapsed tool groups, retain failures explicitly and keep complete command evidence behind the disclosure.

## Proposed Durable Task work panel

M236 supplies work details; [Durable Tasks](architecture/durable_tasks.md#work-panel-and-notifications) owns their behavior.
Present that content through the approved inline Summary and its detail views, without adding a competing sidebar.
Rows show title, current state, next dependency and confirmed result links.

Details expand to local versus published files, checks, attempts, helpers and evidence.

Users keep addressing the fleet without typing task identifiers.
A selected item supplies a validated association hint; unclear references ask which work.
Show queued versus consumed steering and distinguish browser reconnection from a new run.

Show native decision activity with purpose: `Checking your request (Jev)` or `Checking goal progress (Jev)`.
Completion shows the selected action, uncertainty or failure; raw model reasoning stays outside the progress surface.
The CLI and browser use the same bounded states and preserve identity across reconnects.

Show preparation, scanning, upload, verification and restoration separately.
Display acknowledged bytes against known missing bytes, with reused bytes separate and retries deduplicated.
Coalesce attached transfer updates to at most once per second; confirm saved work only after manifest commit.

### Terminal progress

Platform requests show a nine-column ASCII owl in the existing pulse color that flaps and blinks while waiting.
`agentsfleet pet` plays a five-second preview without signing in or contacting the API.
The shared progress renderer uses an Effect scope to stop the animation and clear the line before a result or error.

JSON and redirected output contain no animation.
Set `AGENTSFLEET_REDUCED_MOTION=1` for a static status; continuous integration, `TERM=dumb`, and terminals too narrow for the status also use static text.

### Active chat status motion

Active status text begins muted. A darker shade sweeps left to right across the text over 2.8 seconds, without changing its position, size or opacity. Both shades remain readable against the surface.

Completed and failed messages stay static. Reduced-motion and forced-color preferences use a solid label. This replaces the earlier bright highlight for working-status text (Indy, November 11, 2026).

### Error notices and inline retry

Use a compact, dismissible error notice for immediate feedback, with a small error glyph and short description. Keep the failed operation visible in chat with a quiet underlined **Try again** action and optional diagnostic disclosure. A dismissed notice must not dismiss the failed operation or erase its draft.

Keep retries scoped to the original operation and prevent duplicate submissions. Errors needing access, configuration or sign-in show that specific recovery instead of a retry loop. This follows Indy's November 11, 2026 error reference.


### Review corrections, Nov 12, 2026

The dashboard uses a 19px/24.7px reply heading at weight 650. Attachment names use 12px, with a 36px desktop chip. Navigation glyphs remain 20px, including the bottom control; phones open the shared navigation dialog and desktop expands the rail.

The selected fleet uses neutral raised fill and bold foreground icon; New fleet retains mint emphasis. Account initials use the lavender avatar tokens with 13px bold text.

Desktop operator turns occupy 90% of the reading column; phone turns use its full width. Reply identity has an 8px icon gap. Result cards and loading placeholders share 78px height, 16px corners and a 4px secondary-line gap.

Composer paperclip controls are 36px with an 18px glyph; coarse pointers keep the 44px floor. Keep 4px horizontal text inset, an explicit focused border, and opaque neutral styling for disabled Send. Compare enabled, disabled and focused states individually.

Summary reads the actual pending approval before showing its action and blast radius. Approve and Deny use the existing authenticated decision actions. A decision requires no separate review page.

Missing or failed reads never become an invented target. Run duration and cost come from recorded figures. Quote uses assistant-ui composer state and serializes into the same durable text message the backend stores.

Brand identity tiles retain a dark #151716 surface and mint #b8f0ce rays in both themes. Their radius is 18.75% of their size, matching the approved asset. Rail home, expand, mobile navigation, account and destination controls use neutral hover fill; New fleet retains mint emphasis.


The Usage metric uses the shared 32px role, a 27px line box, weight 750 and normal tracking. This preserves the approved 133px desktop strip. Compact metric rows use 28px values on phones and 26px at intermediate widths.

The chart section keeps its 206px minimum height and 32px gap before the fleet table section; its heading-to-table gap is 16px. Missing daily buckets render an unavailable state within that section.

Saved fleet composers stay docked to the assistant-ui viewport at phone and desktop widths. Empty setup content remains scrollable in normal flow. Phone Summary stays in the conversation flow; the desktop Summary retains a 24px top inset while scrolling.

Comparison captures label top-of-thread and latest positions separately.

### Evidence and interaction corrections, Nov 13, 2026

Form dialogs focus the first enabled, editable field. If the field arrives after loading, Radix retains focus until that initial field is available.

Keyboard or pointer interaction cancels that delayed focus opportunity. Confirmation dialogs focus Cancel.

Preserve native trapping and return focus to the invoking control. Capture keyboard modality consistently and record the active element and `:focus-visible`; an unfocused input alone does not describe dialog focus.

The Changes flap is a centred, content-width raised pill: 38px high, 16px corners, 1px standard border, 12px text and an 8px gap above the composer. Show confirmed file counts; omit line totals without recorded evidence.

Conversation and New fleet composers measure 740 × 139px and 760 × 183px at the approved desktop viewport. Intermediate New fleet layouts retain 32px side padding.

Activity tool rows have a 36px desktop pitch; coarse pointers retain 44px targets. Secret rows use 69px desktop height.

Menu labels use sentence case. Account navigation uses 40px rows beside the rail; workspace switching retains native Radix submenu keyboard behavior.

Archived fleets retain history and disable composer, approval, schedule and task mutations. Deletion evidence shows the destination after deletion, not a toast over the former conversation.

Before packaging, render the unchanged approved proposal and candidate at the same widths and compare computed geometry. Capture 1480px and 390px in both themes, plus Conversation, Models, API Keys and New fleet at 696px and 900px.

Every evidence file identifies a clean candidate revision, source-tree digest and UTC capture time. Motion evidence includes lossless frame pairs for both themes and sizes; reduced-motion pairs remain byte-identical.

The bundle manifest hashes every payload, including reports and its index. Report each pre-submission check as PASS, FAIL or NOT RUN with measurements and filenames; unresolved failures prevent packaging.

The owner-approved account menu has Team, API Keys, Usage, Models, Appearance and Sign out beneath the identity header. The header's native submenu switches and creates workspaces.

API Keys combines keys, sessions and access to the native account/sign-in controls; no separate Personal account destination is shown. Keep password and verified-email operations in the authenticated provider facade in an always-mounted named section.

The header action scrolls/focuses it without changing native verification hashes; do not add an enclosing dialog.
