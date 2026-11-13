# Visual gate for the agentsfleet interface refresh (M249): instructions for Codex

You are finishing the agentsfleet interface refresh. Until now an outside reviewer compared your capture bundles with the approved `proposal.html` and sent findings back. That loop ends here. From now on you run the same review yourself, fix what it finds, and decide the verdict by the rules below. When the verdict is PASS you open the pull request. When something cannot or should not be fixed, you ask Indy for a decision instead of deciding it yourself.

Baseline for this document: candidate `ed7c4032b378ef3c137ec6eeaa48d2f0f665de21`, reviewed three times against proposal SHA-256 `8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a`. All measured values quoted below come from that candidate.

Read this whole file before changing anything.

---

## 1. What PASS means

**VISUAL PASS** means all of the following are true on one clean commit:

1. Every check in the rubric (section 6) is `PASS` or `PASS BY EXCEPTION`.
2. Every item in the open backlog (section 8) is closed with a measured row, or has an exception.
3. Every item in the decision list (section 9) has Indy's answer recorded.
4. The second, adversarial pass (section 10, step 6) found nothing new.
5. No required check is `NOT RUN`.

**PASS BY EXCEPTION** means Indy approved the difference in writing. You record his exact words, the date, and the check it covers in `EXCEPTIONS.md`. You never write an exception on your own authority.

**VISUAL PASS is not release approval.** The repository gates and the authenticated evidence listed in `M249_VERIFICATION.md` are separate. The PR opens only when the visual gate and those gates are both satisfied, or Indy has descoped the missing ones (section 11).

---

## 2. Rules that do not bend

These come directly from mistakes found in the earlier bundles.

1. **Expected values come from the proposal, never from the implementation.** Render `proposal.html` in the same run, at the same width and theme, and read the expected value from it. The earlier self-check expected 40px dialog footer buttons at 390 while `reference-measurements.json` in the same bundle computed 44px. That check passed 128 times and was wrong every time.
2. **A check covers every instance, not a chosen sample.** The earlier primary-button check named four buttons. It missed `Create API key`, which was 40px at 390 against the proposal's 44px. Enumerate every button, field, row, icon and heading in every capture.
3. **Every repair you claim needs its own measured row.** The four Astra repairs had no row in the self-check; they were only asserted in a JSON note. A sentence is not evidence.
4. **Review the commit you ship.** One earlier review ran on `4a88af5e0` and was then cited for a different commit. Captures, measurements, reference render and verdict all carry the same revision and source digest, or the run is void.
5. **Never weaken a check to make it pass.** If you believe a check is wrong, show the reference measurement that proves it, change it, and list the change in the report under "Checks changed". No tolerance widens without Indy.
6. **`NOT RUN` is not `PASS`.** Report it as `NOT RUN` and say why.
7. **Never edit `proposal.html`.** Verify its SHA-256 at the start and end of every run.
8. **Never invent data to match the proposal.** Missing backend facts stay explicitly unavailable.
9. **Screenshots are unmodified.** No cropping, retouching or re-encoding of evidence files.
10. **Do not say Claude, Astra or Indy approved anything they did not approve on this commit.**

---

## 3. Separating real defects from sample content

Most ambiguity in the earlier reviews came from the implementation showing different sample text from the proposal. Table columns, wrapped lines and right-aligned labels all moved, and nobody could tell whether the layout or the text caused it. Remove that ambiguity with two capture modes.

**Parity mode.** A fixture that feeds the production components the proposal's own strings, counts and names for each matched state: "Release watch", "Update runner image", `REGISTRY_TOKEN`, "Kishore’s account", and so on. Fields the backend cannot supply stay unavailable as they are today. In parity mode every geometry check runs at full strictness, because the content is the same.

**Real-data mode.** The current labelled sample data. Here you check tokens, type, icons, control sizes, insets and component structure, and you check that nothing overflows, truncates or collides. Position checks that depend on text length are skipped and marked `CONTENT`.

A difference is `CONTENT` only if it disappears in parity mode. If it remains in parity mode it is a defect. If you cannot build parity mode for a state, say so and treat every unexplained position difference in that state as a defect.

---

## 4. Capture protocol

### Viewports and themes

| Name | Size | Use |
|---|---|---|
| desktop | 1480 × 1000 | Every state |
| wide tablet | 900 × 1000 | Every page and Fleet state, not only four |
| narrow tablet | 696 × 1000 | Every page and Fleet state, not only four |
| phone | 390 × 844 | Every state |
| phone, short | 390 × 500 | Composer focused |
| phone, touch | 390 × 844 with touch enabled and a coarse pointer | Every state with a button |

Device scale 1. Zoom 100%. Dark and light for every capture. Fonts loaded before capture. Pointer parked away from interactive elements unless the state is a hover state; an earlier capture showed an accidental hover highlight on "Use for new fleet".

The touch run is required. The proposal's 44px phone control height is triggered by width. If the implementation ties it to pointer type, a 390 capture with a mouse pointer shows 40px and nobody can tell whether a real phone gets 44px.

### Review bar offset

The proposal draws its own review bar above the app. It is one row at 1480 and 900, so the shell starts at y46. It is two rows at 696 and 390, so the shell starts at y80. The implementation harness bar is always one row. **Compare shell-relative coordinates**: subtract the shell's top from every y value on both sides. Also take one set of captures with the harness bar removed, to check anything positioned against the viewport such as toasts.

### State matrix

Pages: Models, Team, Usage, API Keys, Secrets, Plugins, Connected apps, New fleet. On phone add a scrolled-to-bottom capture of each page that scrolls.

Fleet states: Conversation (top and latest), Activity, Wake-up (top and latest), Error, Working, Archived, Deleted destination, Account menu open, Composer focused.

Dialogs: Create model (new key, saved key, and scrolled to the footer on phone), Model details, Edit model, Invite member, Create API key, Revoke API key, Sign out session, Sign out other sessions, Add secret, Manage secret, Replace secret, Delete secret, Plugin details, Connected app details, Archive fleet, Delete fleet, Recorded changes.

Interaction states: primary button hover, primary button keyboard focus, search focus, one keyboard-focused control inside a dialog.

**New required states:**

- A secret whose name is at least 64 characters with no spaces, and one of about 40 characters with spaces, in Manage secret, Replace secret and Delete secret, at 1480 and 390, both themes. The complete name must be readable somewhere in each dialog.
- Team, Plugins and Usage with at least three rows, so row rhythm can be measured.
- Loader: eight recordings and eight lossless frame pairs (1480 and 390, both themes, motion allowed and reduced), with the recording cropped or annotated so the label's box is known.

### Reference render

Open `proposal.html` from disk in the same browser build. Set the theme with `document.documentElement.dataset.theme`. Change view with the page's own `showView(name)`. Open dialogs by clicking the proposal's own controls.

| Implementation state | Proposal view or action |
|---|---|
| conversation | `conversation` |
| activity | `activity` |
| wake-up | `scheduled` |
| error | `failure` |
| new-fleet | `empty` |
| models, team, usage, secrets, plugins | same name |
| API Keys | `security` |
| connected-apps | `connections` |
| create-model | `[data-action=create-model]`; switch the radio for the saved-key variant |
| invite-member | `[data-team-action=invite]` |
| create-api-key | `[data-action=create-key]` |
| revoke-api-key | `[data-revoke-key]` |
| sign-out-session | `[data-revoke-session]` |
| sign-out-other-sessions | `[data-action=revoke-other-sessions]` |
| add-secret | `[data-action=add-secret]` |
| plugin-details | `[data-plugin]` |
| connected-app-details | `[data-action=connection-details]` |
| archive-fleet, delete-fleet | `#fleet-menu [data-action=archive]` and `[data-action=delete]` |
| recorded-changes | `[data-action=changes]` and `[data-action=activity-changes]` |
| account-menu | click `#account-toggle` |
| fleet-archived | confirm the archive dialog, then open the archived fleet |
| fleet-deleted-destination | confirm the delete dialog |

No proposal equivalent exists for Model details, Edit model, Manage secret, Replace secret, Delete secret and Working. Judge those against the shared rules in section 6 (dialog frame, fields, footer, focus, type, tokens, spacing rhythm) and against the nearest proposal pattern.

---

## 5. How to measure

Use three methods together. Computed styles alone missed several defects that pixels showed.

### 5.1 Element geometry and computed style

For every visible element of interest on both sides record: role, accessible name, text, bounding box, font family, size, weight, line height, colour, background, border width and colour, radius, padding, gap, box shadow, outline, and whether it has keyboard focus. Match elements between proposal and implementation by role and name, not by index.

### 5.2 Ink bounds from pixels

For text and icons, measure where the drawn pixels are, not where the box is. Take a band of the screenshot, find its most common colour as background, and list the runs of rows or columns that differ from it.

```python
import numpy as np
from PIL import Image

def ink_runs(path, box, axis, threshold=30, min_gap=2):
    """Runs of rows (axis=1) or columns (axis=0) containing ink inside box=(x0,y0,x1,y1)."""
    a = np.asarray(Image.open(path).convert("RGB")).astype(int)
    x0, y0, x1, y1 = box
    region = a[y0:y1, x0:x1]
    colours, counts = np.unique(region.reshape(-1, 3), axis=0, return_counts=True)
    background = colours[counts.argmax()]
    ink = (np.abs(region - background).sum(axis=2) > threshold).any(axis=axis)
    runs, start, last = [], None, None
    for i, on in enumerate(ink):
        if on:
            start = i if start is None else start
            last = i
        elif start is not None and i - last > min_gap:
            runs.append((start, last)); start = None
    if start is not None:
        runs.append((start, last))
    offset = y0 if axis == 1 else x0
    return [(s + offset, e + offset) for s, e in runs]
```

This is how the header gaps, the day marker, the file chip, the output card title, the Summary rhythm, the rail offsets and the account-menu insets were found. Use it wherever two things should line up or sit a fixed distance apart.

### 5.3 Theme parity from edges

Dark and light must have the same layout. Build an edge mask of each capture (neighbouring pixels differing by more than 14 grey levels), ignore the harness bar, dilate by 2px, and count edge pixels in one theme with no neighbour in the other. At `ed7c4032b` the worst pair differed by 0.7%. Fail any pair above 1.5%.

### 5.4 Pixel checks for states

- **Hover:** sample the button fill in the normal and hover captures. The ratio must be 0.94 on each channel, within 0.01.
- **Focus ring:** scan outward from the control's edge. Expect 4px of the surface the control sits on, then 2px of accent, then the surface again. Inside a dialog the 4px must be the dialog's own background.
- **Icon identity:** compare the icon's pixels with the same Phosphor glyph rendered at the same size, or compare the SVG path. Two different actions must not share a glyph unless the proposal does.

### 5.5 Look at it

After the numbers, view proposal and implementation side by side at 100% for every state, and at 2× to 5× for every dialog, header, list row and icon. Ask the questions in section 7. Several findings (wrong icon, two-tone focus halo, stacked phone buttons, a wrapped "Done" button) were seen before they were measured.

### Tolerances

| Property | Tolerance |
|---|---|
| Position and size | ±1px, shell-relative |
| Font family, size, weight, line height | Exact |
| Colour of text, fill, border, ring | Exact computed value |
| Radius, border width, ring width and offset | Exact |
| Control height | Exact |
| Icon glyph | Exact |
| Hover brightness | 0.94 ± 0.01 |
| Theme edge parity | ≤ 1.5% |

The wake-up strip is the one approved numeric exception: 45px against the proposal's 46px.

---

## 6. Rubric

Each check gets a row in `self-check.md`: check id, capture filename, expected value and where it came from, measured value, result. Run every check at every width and in both themes unless it says otherwise.

### A. Evidence

| Id | Check | Pass when |
|---|---|---|
| A1 | Clean source | `git status` empty at capture start and end; same HEAD and source digest |
| A2 | Proposal unchanged | SHA-256 equals the approved value at start and end |
| A3 | Manifest | Every file listed with matching size and SHA-256; nothing unlisted except the manifest |
| A4 | One revision | Every capture, measurement, reference file and report carries the same revision and digest |
| A5 | Matrix complete | Every state in section 4 exists at every required width, theme and pointer type |
| A6 | No duplicates | No two state images share a hash, apart from reduced-motion loader pairs |
| A7 | No page overflow | No horizontal scroll on the page at any width |
| A8 | No console errors | Zero during capture |

### B. Tokens and colour

Expected values, dark then light:

| Token | Dark | Light |
|---|---|---|
| Canvas | `#101114` | `#e9ebee` |
| Surface (shell, page) | `#181a1e` | `#fafbfc` |
| Panel (cards, dialogs, composer, Summary) | `#202328` | `#f0f2f5` |
| Raised (user bubble) | `#2b2e35` | `#e3e7ec` |
| Text | `#f5f6f8` | `#20242b` |
| Secondary text | `#c2c7d0` | `#495361` |
| Muted text | `#a3abb7` | `#5e6876` |
| Line | `#3d424b` | `#bdc5ce` |
| Soft line | `#30343c` | `#d7dce2` |
| Accent | `#c1f0d6` | `#24593d` |
| Radius | 10, 16, 22px | same |

| Id | Check | Pass when |
|---|---|---|
| B1 | Surfaces | Canvas, shell, header, bubble, composer, Summary, cards and dialogs sample to the token above |
| B2 | Borders | Shell, cards, composer, Summary, fields and dialogs use the proposal's line colour and 1px width |
| B3 | Text roles | Each text element uses the same role colour as its proposal counterpart |
| B4 | Radius | Controls 10px, cards and flap 16px, shell and dialogs 22px |
| B5 | Theme parity | Section 5.3 passes for every pair |

### C. Typography

| Id | Check | Expected |
|---|---|---|
| C1 | Family | Manrope for interface text; the monospace role only for identifiers, commands, file names, secret names and credential fields |
| C2 | Desktop reading text | 13px / 20px, weight 400 |
| C3 | Metadata and labels | 12px / 16px |
| C4 | Phone reading and typing | 16px; form fields and the composer 16px at 390 |
| C5 | Page title | 20px / 26px, weight 650 |
| C6 | Dialog title | 18px, weight 650, in a 40px row |
| C7 | Assistant heading | 19px / 24.7px, weight 650 |
| C8 | Shell header title | 13px / 20px, weight 600 |
| C9 | New fleet heading | 34px at 1480 and 900, 28px at 696, weight 650 |
| C10 | Field labels and button text | Weight 500 |
| C11 | Section asides | 13px in the secondary colour on every page |
| C12 | Case and quotes | Sentence case; curly quotes around named targets; no uppercase eyebrow labels |

### D. Icons

| Id | Check | Pass when |
|---|---|---|
| D1 | Glyph identity | Every icon is the same Phosphor glyph as its proposal counterpart |
| D2 | Size | 20px in the rail, 16px in controls and rows, 18px in the composer |
| D3 | Alignment | An icon beside two lines of text is centred on both lines, as in the proposal's Summary rows |
| D4 | Distinct actions | Two different actions in one view never share a glyph unless the proposal does |
| D5 | Disclosure | Phosphor caret, never the browser's solid triangle |

### E. Controls

| Id | Check | Expected |
|---|---|---|
| E1 | Button height, 696 and wider | 40px; fields and selects 44px |
| E2 | Button height at 390 | 44px for **every** button on a page and in a dialog footer; row menu buttons 44 × 44 |
| E3 | Compact controls | Only where the proposal is compact (the Secrets row `Manage` is 36px) |
| E4 | Width | Same-label buttons within 1px of the proposal |
| E5 | Field padding | 12px horizontal |
| E6 | Hover | Section 5.4 |
| E7 | Keyboard focus | 2px accent ring, 4px offset; search container 2px offset; the gap shows the surface behind the control |
| E8 | Composer focus | Border becomes the secondary text colour; no ring |
| E9 | Disabled | Neutral fill, as approved for Send, New fleet, Create model and Save |
| E10 | Tooltips | None on a control with a visible label |

### F. Dialogs

| Id | Check | Expected |
|---|---|---|
| F1 | Width | 540px; Create model 620px; 358px at 390 |
| F2 | Padding | 24px |
| F3 | Title row | 40px; close button 40 × 40 with its right edge 17px inside the dialog |
| F4 | Footer gap | 24px from content to the first footer button |
| F5 | Footer layout | Buttons in one right-aligned row with 8px gaps **at every width including 390**; no stacking; no button wrapping alone onto a second row |
| F6 | Internal rhythm | 8px between stacked options, disclosures and buttons; no 0px joins |
| F7 | Focus clearance | A focused control's ring keeps at least 6px from the next control |
| F8 | Focus ring gap colour | The dialog's own background, not the page surface |
| F9 | Initial focus | First editable field in a form; Cancel in a confirmation; never a destructive action |
| F10 | Named targets | The complete name is readable in the dialog at every width; a truncated title is acceptable only when the body or a field shows the name in full |
| F11 | Description lists | Labels left, values right-aligned to the content edge, even row pitch |

### G. Shell and navigation

| Id | Check | Expected |
|---|---|---|
| G1 | Rail | 48 × 48 buttons; logo and navigation toggle at the proposal's y positions |
| G2 | Header inset | Icon and title at x98 / x125 at 1480, x90 / x117 at 900, x86 / x109 at 696; no leading icon at 390 |
| G3 | Header gaps | Title, menu button, status and cost spaced as in the proposal; cost right-aligned to the same offset from the New fleet shortcut |
| G4 | Header actions | The New fleet shortcut is present in every Fleet state, including Archived |
| G5 | Account menu | 40px rows; item icon and label insets equal to the proposal; approved entries: workspace header, Team, API Keys, Usage, Models, Appearance, Sign out |
| G6 | Toasts | Anchored to the shell header band, not the viewport top |

### H. Resource pages

For each of Models, Team, Usage, API Keys, Secrets, Plugins, Connected apps and New fleet:

| Id | Check | Pass when |
|---|---|---|
| H1 | Heading block | Title, description and primary action at the proposal's shell-relative positions at every width |
| H2 | Page top padding | Equal to the proposal at every width (it shrinks at 900 and below) |
| H3 | Section rhythm | 24px between page sections; section title, count chip and aside placed as in the proposal |
| H4 | Cards and rows | Same padding, row height, icon tile and divider as the proposal; at 390 card text is inset 16px |
| H5 | Tables | Header and row heights equal; columns checked in parity mode |
| H6 | Wrapping | Text wraps as the proposal's does; nothing is cut with an ellipsis where the proposal wraps or shows it in full |
| H7 | Filters | A select is wide enough for its own default label |

### I. Fleet conversation

| Id | Check | Expected at 1480 |
|---|---|---|
| I1 | Composer | 740 × 139 at x250, y808; 760 × 183 on New fleet; x97 w510 at 900; x89 w342 at 696 |
| I2 | Summary | x1216 w214; x664 w194 at 900; x472 w190 at 696; above the transcript at 390 |
| I3 | Changes flap | 38px high, 16px radius, centred on the composer, 8px above it |
| I4 | Wake-up strip | x270 to x970, 45px, attached to the composer; action controls at the proposal's 36px spacing |
| I5 | Day marker, bubble, file chip | Shell-relative positions and sizes equal to the proposal |
| I6 | Reply block | Sender line, text, tool group, heading and paragraph within 1px |
| I7 | Tool rows | 36px pitch; outcome glyphs in the secondary colour; targets in monospace |
| I8 | Output card | Title and subtitle positions equal to the proposal; subtitle wraps |
| I9 | Summary rows | Label-to-row distance, icon centring and arrow size equal to the proposal's rows |
| I10 | Message actions | Copy and Quote in the same plain style in every state |
| I11 | Phone model selector | Label shown in full; the action button wraps below when space is short |

### J. Fleet states

| Id | State | Pass when |
|---|---|---|
| J1 | Activity | Flap present; rows as I7 |
| J2 | Wake-up | Strip as I4; event rows match the proposal's event style |
| J3 | Error | Error card colours, border, radius, title and spacing equal to the proposal; one recovery action; one message |
| J4 | Working | Running command in monospace; loader as section L |
| J5 | Archived | Header state drawn as the proposal's neutral dot and text; footer notice with divider and archive icon; no mutation controls |
| J6 | Deleted | Lands on New fleet with the toast in the header band |

### K. Responsive

| Id | Check | Pass when |
|---|---|---|
| K1 | 900 and 696 | Every page and Fleet state matches the proposal's shell-relative layout |
| K2 | 390 | Summary above transcript; status hidden; cost hidden; controls as E2 |
| K3 | 390 × 500 | Focused composer and its controls fit without overlap |
| K4 | No collisions | Floating controls do not sit on top of text in a resting state |
| K5 | Touch run | Section E passes in the touch captures |

### L. Loader

| Id | Check | Pass when |
|---|---|---|
| L1 | Motion pairs | Each lossless pair differs by at least 300 pixels inside the label's box |
| L2 | Reduced pairs | Byte-identical |
| L3 | Sweep | In each motion recording the darker band crosses the label and repeats at a steady period (about 3 seconds at the baseline) |
| L4 | Reduced recordings | No pixel inside the label's box changes over the steady part of the recording |
| L5 | Resting colour | The label's base colour is the same with motion allowed and reduced, in both themes |

Measure L3 and L4 inside the label's box only. Whole-frame measurements were confused by other things moving, such as the composer caret.

---

## 7. Questions to ask while looking

Use these on every capture. They are how the non-numeric findings were found.

1. Does anything touch, overlap or nearly touch that has room in the proposal?
2. Is any text cut off? Would it be cut off with a longer real name?
3. Do two neighbouring things that should match (two buttons, two rows, two icons) actually match in height, inset, weight and glyph?
4. Is the spacing between stacked items even, or does one pair sit closer than the rest?
5. With a focus ring showing, does the ring look like one clean line, or is there a second band or halo?
6. Is the destructive action ever the default or the first focused?
7. On phone, could a thumb hit every control? Is any row of buttons broken across lines?
8. Does the header carry the same items in every state of the same screen?
9. Does the light capture have any element that vanishes, loses its border or changes weight?
10. Is anything present that the proposal does not have, or missing that it does? If so, is it in the approved list?

---

## 8. Open backlog from the three reviews

Fix each item, add its regression check, and show the measured row. "Proposal" values are from the reference render; "Measured" values are from `ed7c4032b`. Captures are dark unless stated; the defects exist in both themes.

### P2

| # | Where | Proposal | Measured | Check |
|---|---|---|---|---|
| 1 | Confirmation dialogs at 390: revoke API key, sign out session, sign out other sessions, delete secret, archive fleet, delete fleet | Cancel and the action inline, right-aligned, 8px apart | Two stacked full-width 308px buttons | F5 |
| 2 | Every dialog footer at 390 | Buttons 44px | 40px | E2 |
| 10 | In-page buttons at 390 on API Keys, Connected apps, Models, Team | 44px (`Create API key`, `Revoke`, `Sign out`, `Sign out other sessions`, `Manage`, `Make active`, `Add another model`, `Use this prompt`); row menus 44 × 44 | 40px; row menus 40 × 40 | E2, K5 |

### P3

| # | Where | Proposal | Measured | Check |
|---|---|---|---|---|
| 3 | Focus ring in every dialog | 4px gap shows the dialog background | Gap painted `rgb(24,26,30)` on `rgb(32,35,40)`; light `250,251,252` on `240,242,245` | F8 |
| 4 | Plugin details | Edit setup uses the simple pencil; Use for new fleet uses the note-pencil | Both use the note-pencil | D1, D4 |
| 5 | Plugin details | 8px between stacked items | Inspect setup to Delete Plugin is 0px | F6 |
| 6 | Manage secret at 390 | One footer row | Done wraps alone to a second row | F5 |
| 8 | Rail | Navigation toggle 48 × 48 at y876; logo ink y72 to 87 | 48 × 44 at y880; logo y73 to 90 | G1 |
| 9 | Edit model | Ring clear of the next control | 8px gap leaves 2px between ring and toggle | F7 |
| 11 | Summary rows | About 20px from label to row; icon and arrow centred on two lines; arrow about 10px | Outputs 15px, Sources 14px, Next wake-up 9px; icon on the first line; arrow about 6px at the top | I9, D3 |
| 12 | Conversation header | Menu x242, status dot x279, status text x293; cost ends x1386; title weight 600 | x239, x272, x286; x1398; weight 650 | G3, C8 |
| 13 | Conversation | Day marker top y146; file chip ends x492; output card title top y568 | y150; x500; y571 | I5, I8 |
| 14 | Wake-up strip | Action icons at x865, 901, 937 | x855, 895, 935 | I4 |
| 15 | Account menu | Usage uses the bar chart; item icon x102, label x129; avatar x101 | Line chart; x96, x119; x97 | D1, G5 |
| 16 | Plugins | "Current account" 13px, secondary colour | 12px, muted colour | C11 |
| 17 | Archived | Dot and "Archived" text in the header; footer notice with divider and archive icon; archive icon in the toast | Bordered chip; no divider or icon; check icon; New fleet shortcut missing; Quote has a filled background | J5, G4, I10 |
| 18 | Archived and Deleted toasts at 1480 | About y51 to 90, in the header band | About y6 to 47, above the shell | G6 |
| 19 | Cards at 390 | Text inset 16px (x90) | 20px in list rows (x94); 24px in Platform default (x98) | H4 |
| 20 | New fleet at 390 | Model label in full; button wraps below; no leading header icon | "Authoring mod…"; leading icon shown (also on Conversation, Working, Wake-up) | I11, G2 |
| 21 | Pages at 900 | Page heading y143 | y159 | H2 |
| 22 | Shell header at 900 and 696 | Icon and title x90 / x117 and x86 / x109 | x98 / x125 at every width; New fleet title x121 at every width; Conversation title x113 at 696 | G2 |
| 23 | Usage | Fleet filter shows its label | "All fleets & setup…" in a 160px select | H7 |
| 24 | Team at 1480 | Row menu 36 × 36 at x1235 | 40 × 40 at x1231; Role and Date columns 4px left | E3 |
| 26 | Output card at 696 and 390 | Subtitle wraps | Cut with an ellipsis | H6, I8 |

### To confirm

| # | Where | Note |
|---|---|---|
| 27 | Secrets row `Manage` at 390 | The supplied metrics show 65 × 36 in the proposal and 83 × 36 in the implementation. Not confirmed from pixels. Measure it and fix it if it holds in parity mode. |
| 28 | Long secret names | Never captured. Add the states in section 4 and apply F10. |

---

## 9. Decisions only Indy can make

Do not fix and do not accept these on your own. Send them as one request (section 11) and wait.

| # | Question | What the proposal does | What the implementation does |
|---|---|---|---|
| D1 | Changes flap on Conversation | Flap on Activity only; Conversation shows changes in the Summary | Flap on both |
| D2 | Phone composer | Scrolls with the page | Pinned to the bottom |
| D3 | Create model footer buttons at 1480, 900, 696 | 44px | 40px, like other dialogs |
| D4 | Create model at 390 | 20px padding; footer pinned with a top divider | 24px padding; no divider (backlog item 7) |
| D5 | Loader label in light | No loader animation | Animated label starts from `rgb(94,104,118)`; static label is `rgb(73,83,97)` |
| D6 | Header status on Activity and Working | "Ready to review" on Activity | No status |
| D7 | Archived Summary | Status section explaining the fleet cannot resume | Regular sections, no Status |
| D8 | Usage selects at 390 | Smaller than 16px | 16px after the earlier phone-select fix |
| D9 | Strip and Team row controls | 36px | 40px (backlog 14 and 24), if you want the larger target kept |

Already approved, do not reopen: Security renamed API Keys with profile and sign-in settings; workspace submenu in the account menu; inline Approve and Deny; neutral empty Send and New fleet; no Slack receipt line, mention button, token budget, diff totals, daily chart, key suffix or last-used time; More filters on Usage; Delete Plugin and Inspect setup in the plugin dialog; Show advanced models in the model pickers; Sources and Tasks in the Summary; wake-up strip 45px.

---

## 10. The loop

1. **Start clean.** Fetch, confirm the branch and HEAD, confirm `git status` is empty, verify the proposal hash.
2. **Build the tools once.** Parity fixtures, reference renderer, capture script, measurement extractor, the checks in section 6. Commit them with the work.
3. **Fix the backlog.** P2 first. Fix shared components rather than single screens: items 2 and 10 are one rule, items 21 and 22 are one layout rule, item 3 is one token.
4. **Capture** the full matrix from the clean commit, plus the reference render, in one run.
5. **Run the rubric.** Write `self-check.md` and `self-check.json`. Any `FAIL` sends you back to step 3.
6. **Adversarial pass.** With a fresh context, or a sub-agent that has not seen your fixes, give it only the captures, the reference render and sections 5 to 7. Its job is to find defects, not to confirm closure. It must view every state side by side and report at least what it checked per screen. Anything it finds goes into the backlog and you return to step 3.
7. **Regression sweep.** After the last fix, re-run everything. A fix to a shared component can move something that passed.
8. **Stop conditions.** Stop and write to Indy if the same check fails after three fix attempts, if a fix would change behaviour, permissions or data, or if two rubric rows contradict each other.
9. **Verdict.** Apply section 1.

Never batch "small" differences as acceptable. Below 2px on a single element is within tolerance only if the check's tolerance says so; anything else is fixed or goes to Indy.

---

## 11. Asking Indy

Use this when a check cannot pass, a decision in section 9 is open, or a required gate cannot run. One message, every item, no approval implied.

```
Subject: M249 interface refresh: decisions needed before the PR (commit <sha>)

Visual gate: <n> checks PASS, <n> FAIL, <n> NOT RUN.

Decisions needed
1. <short title>
   Proposal: <value, capture or reference file>
   Implementation: <value, capture filename>
   Why not fixed: <reason>
   Options: (a) match the proposal, (b) keep as is, (c) remove
   My recommendation: <one line>

Gates I could not run
1. <gate>: <what blocked it> — may I open the PR without it?

Reply per item with a, b or c. I will record your words in EXCEPTIONS.md.
```

Record each answer in `EXCEPTIONS.md` as: date, item, Indy's exact words, the check ids it covers, the commit it applies to. An exception covers only what it names. If the code it covers changes, the exception lapses.

---

## 12. Opening the PR

Open it only when all of these hold on the same commit:

1. VISUAL PASS by section 1.
2. The repository gates in the `M249_VERIFICATION.md` punchlist have run and report their real results: lint, unit tests, coverage at the enforced thresholds, the rendered-flow run, the acceptance run, and the app and website bundle-size checks.
3. Authenticated evidence on this commit: account controls, a real archive with read-only history, a real delete landing on New fleet. If you cannot produce it, ask Indy to descope it; do not substitute fixture images.
4. `EXCEPTIONS.md` is complete.

Then commit, push, open the PR and **do not merge**. Request Indy's review. The PR description contains:

- Commit, source digest, proposal hash.
- Verdict line: `VISUAL PASS` or `VISUAL PASS WITH EXCEPTIONS (<n>)`.
- Counts: checks passed, exceptions, not run.
- The closed backlog with before and after values.
- The exceptions with Indy's words.
- Every gate with its actual result.
- "Not verified" list.
- Where the capture bundles and reports are.

If any condition fails, do not open the PR. Report what failed.

---

## 13. Report files for each run

| File | Contents |
|---|---|
| `STATUS.md` | Commit, digest, verdict, counts, what is pending |
| `self-check.md`, `self-check.json` | One row per check per capture |
| `reference-measurements.json` | Proposal geometry for every view, width and theme in this run |
| `measurements.json` | Implementation geometry, parity and real-data modes labelled |
| `BACKLOG.md` | Every item in section 8 and anything found since, with status and evidence |
| `EXCEPTIONS.md` | Indy's decisions |
| `ADVERSARIAL_REVIEW.md` | What the second pass checked and found |
| `manifest.json` | Size and SHA-256 of everything |

Label estimates as estimates. Say what you did not check. A short honest "not verified" list is worth more than a long green table.
