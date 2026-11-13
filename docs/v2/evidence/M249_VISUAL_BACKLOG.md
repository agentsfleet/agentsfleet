# M249 visual repair backlog

The ledger has 32 retained rows, numbered up to 34. Item 25 was folded into 22; item 7 is absent from the supplied list.

These identifiers are not a count of outstanding source changes. Repairs already implemented still need verification before their rows close.

Every retained finding requires closure before M249_001 moves to done. Accepted decisions are recorded in M249_EXCEPTIONS.md.

Source repairs and diagnostic results are recorded in M249_VERIFICATION.md. They do not imply final verification has passed.

Indy stopped additional ZIPs and full capture delivery. Use targeted internal measurements and native regression checks; Indy reviews the PR preview or deployment.

Backend data differences belong to [M249_004](../pending/M249_004_P1_API_DOCS_UI_RECORDED_DATA_COMPLETION.md), not this visual backlog.

## Current verification status

✅ 20 of 32 findings are verified. The remaining 12 are: 8, 9, 10, 11, 14, 15, 17, 18, 23, 24, 29, 33.

The native v237 run passed 4,564 app tests and all 90 browser checks. Model details now uses its native accessible button and expanded state.

The integrated #838 checkpoint `11f2d4ffd` produced 416 same-run measurement records with no browser errors. All 48 confirmation cases and 136 dialog footer cases passed. Changes to subtitle type and phone composer width require their own follow-up measurements.

## Owner findings

## 8. Open backlog from the three reviews

Fix each item, add its regression check, and show the measured row. "Proposal" values are from the reference render; "Measured" values are from `ed7c4032b`. Captures are dark unless stated; the defects exist in both themes.

### P2

| # | Where | Proposal | Measured | Check |
|---|---|---|---|---|
| ✅ 1 | Confirmation dialogs at 390: revoke API key, sign out session, sign out other sessions, delete secret, archive fleet, delete fleet | Cancel and the action inline, right-aligned, 8px apart | Two stacked full-width 308px buttons | F5 |
| ✅ 2 | Every dialog footer at 390 | Buttons 44px | 40px | E2 |
| 10 | In-page buttons at 390 on API Keys, Connected apps, Models, Team | 44px (`Create API key`, `Revoke`, `Sign out`, `Sign out other sessions`, `Manage`, `Make active`, `Add another model`, `Use this prompt`); row menus 44 × 44 | 40px; row menus 40 × 40 | E2, K5 |

### P3

| # | Where | Proposal | Measured | Check |
|---|---|---|---|---|
| ✅ 3 | Focus ring in every dialog | 4px gap shows the dialog background | Gap painted `rgb(24,26,30)` on `rgb(32,35,40)`; light `250,251,252` on `240,242,245` | F8 |
| ✅ 4 | Plugin details | Edit setup uses the simple pencil; Use for new fleet uses the note-pencil | Both use the note-pencil | D1, D4 |
| ✅ 5 | Plugin details | 8px between stacked items | Inspect setup to Delete Plugin is 0px | F6 |
| ✅ 6 | Manage secret at 390 | One footer row | Done wraps alone to a second row | F5 |
| 8 | Rail | Navigation toggle 48 × 48 at y876; logo ink y72 to 87 | 48 × 44 at y880; logo y73 to 90 | G1 |
| 9 | Edit model | Ring clear of the next control | 8px gap leaves 2px between ring and toggle | F7 |
| 11 | Summary rows | About 20px from label to row; icon and arrow centred on two lines; arrow about 10px | Outputs 15px, Sources 14px, Next wake-up 9px; icon on the first line; arrow about 6px at the top | I9, D3 |
| ✅ 12 | Conversation header | Menu x242, status dot x279, status text x293; cost ends x1386; title weight 600 | x239, x272, x286; x1398; weight 650 | G3, C8 |
| ✅ 13 | Conversation | Day marker top y146; file chip ends x492; output card title top y568 | y150; x500; y571 | I5, I8 |
| 14 | Wake-up strip | Action icons at x865, 901, 937 | x855, 895, 935 | I4 |
| 15 | Account menu | Usage uses the bar chart; item icon x102, label x129; avatar x101 | Line chart; x96, x119; x97 | D1, G5 |
| ✅ 16 | Plugins | "Current account" 13px, secondary colour | 12px, muted colour | C11 |
| 17 | Archived | Dot and "Archived" text in the header; footer notice with divider and archive icon; archive icon in the toast | Bordered chip; no divider or icon; check icon; New fleet shortcut missing; Quote has a filled background | J5, G4, I10 |
| 18 | Archived and Deleted toasts at 1480 | About y51 to 90, in the header band | About y6 to 47, above the shell | G6 |
| ✅ 19 | Cards at 390 | Text inset 16px (x90) | 20px in list rows (x94); 24px in Platform default (x98) | H4 |
| ✅ 20 | New fleet at 390 | Model label in full; button wraps below; no leading header icon | "Authoring mod…"; leading icon shown (also on Conversation, Working, Wake-up) | I11, G2 |
| ✅ 21 | Pages at 900 | Page heading y143 | y159 | H2 |
| ✅ 22 | Shell header at 900 and 696 | Icon and title x90 / x117 and x86 / x109 | x98 / x125 at every width; New fleet title x121 at every width; Conversation title x113 at 696 | G2 |
| 23 | Usage | Fleet filter shows its label | "All fleets & setup…" in a 160px select | H7 |
| 24 | Team at 1480 | Row menu 36 × 36 at x1235 | 40 × 40 at x1231; Role and Date columns 4px left | E3 |
| ✅ 26 | Output card at 696 and 390 | Subtitle wraps | Cut with an ellipsis | H6, I8 |

### To confirm

| # | Where | Note |
|---|---|---|
| ✅ 27 | Secrets row `Manage` at 390 | The supplied metrics show 65 × 36 in the proposal and 83 × 36 in the implementation. Not confirmed from pixels. Measure it and fix it if it holds in parity mode. |
| ✅ 28 | Long secret names | Never captured. Add the states in section 4 and apply F10. |

---


## Follow-up findings

| Item | Scope | Required evidence | Status |
| --- | --- | --- | --- |
| 29 | Text weight, role colour and browser font smoothing across chat and Summary | Same-run computed styles, matching font asset hashes and pixel/ink checks | ◻ Verification pending |
| 30 | Four corner radii and header cost type | Per-component radii; 13px/20px/500 cost label and baseline | ✅ Verified — v253 |
| 31 | Exact action inspection | Shared dialog, preserved reason, inline approval decisions | ✅ Verified — v237 |
| 32 | Generic activity summaries | Targets, raw commands, tool names and duration appear inside folds | ✅ Verified — v237 |
| 33 | Activity connectors and separators | Vertical connectors in open details; no outer horizontal rules in open activity | ◻ Verification pending |
| 34 | Cancel alignment | Every footer: same height/baseline, right alignment and 8px gap | ✅ Verified — v253 |

See M249_EXCEPTIONS.md for the nine accepted decisions. No external review ZIP is required.

## Measured closures after chat integration

Evidence: `/workspace/artifacts/fleet-refresh-v2/backlog-points-v253/point-measurements.json` and `finding-results.json`. Revision `11f2d4ffd`; proposal SHA-256 remains unchanged. Both themes and all four widths were measured. The test fixture matches the reference review-bar height; product content styles are unchanged.

| Finding | Result | Measured evidence |
|---|---|---|
| ✅ 1 | PASS | 48 confirmation cases; no stacking; 8px gaps. |
| ✅ 2 | PASS | Every measured phone footer is 44px. |
| ✅ 3 | PASS | Focused ring offsets use #202328 dark / #f0f2f5 light. |
| ✅ 5 | PASS | Edit, Use, Inspect and Remove have 8px gaps. |
| ✅ 6 | PASS | Replace/Delete are contextual body actions; Done is the single footer action. |
| ✅ 12 | PASS | Menu x226.844; title 600; cost text ends x1387, baseline bounds y69.5–87.5. |
| ✅ 13 | PASS | Day marker y143; chip right493.297; output heading ink y476. |
| ✅ 16 | PASS | 13px; correct secondary colour in both themes at all four widths. |
| ✅ 19 | PASS | Models, Team, API Keys, Connected apps and Plugins text x90. |
| ✅ 20 | PASS | Full Authoring model label; no header icon; composer 288 × 227 matches reference. |
| ✅ 21 | PASS | All seven resource page heading offsets match within 1px. |
| ✅ 22 | PASS | Title x69 at390;109 at696;117 at900. |
| ✅ 26 | PASS | Repository metadata uses normal wrapping and no ellipsis; weight repair tracked separately. |
| ✅ 27 | PASS | 64.953 × 36 in both themes. |
| ✅ 28 | PASS | 24 long-name states; spaced/unbroken; Manage/Replace/Delete; no clipped target or page overflow. |
| ✅ 30 | PASS | Chip 10px; bubble22/22/6/22; Summary 22px; composer 22px; cost 13px/20px/500. |
| ✅ 34 | PASS | Every measured dialog footer is aligned with equal button heights and 8px gaps. |
| ✅ 4 | PASS | PencilSimple for Edit; NotePencil for New fleet; distinct 16px rendered glyphs. |

Finding 8 is only partly measured: the toggle is 48 × 48 at y876; the logo ink check remains open. Findings 29 and 11 remain open while their typography repairs are verified. No native final-gate result is claimed here.

## Verified finding evidence

| Finding | Evidence | Verified result |
|---|---|---|
| ✅ 31 | `FleetApprovalSummary.test.tsx`: “keeps evidence and a reason in the conversation and retains authorization errors”; passed within v237 app units | Exact action opens the shared dialog; recorded evidence is shown; the reason survives Done; Approve submits it and retains a 403 without claiming success. |
| ✅ 32 | `chat-typography.spec.ts`: “recorded actions stay visible and distinguish every terminal outcome”; passed on desktop and phone in v237 | Visible summaries are generic; the full target remains accessible; raw input and duration appear inside details; success, failed, interrupted and unknown outcomes stay distinct. |

Log: `/workspace/scratch/refresh-dry-app-v237.log`. Production code: `f12b9e326`, with the repaired resource selectors. The evidence records that test and documentation changes were uncommitted during the run.
