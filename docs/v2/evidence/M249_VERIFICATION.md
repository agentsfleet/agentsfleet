# M249 interface refresh: repairs and verification

This report tracks the approved proposal, the final Claude review and the implemented repairs.
It distinguishes reviewed visuals, source fixes, automated checks and owner approval.
The UI refresh and the pending CLI work ship in separate PRs.

## Current result

The independent Astra review found no remaining P2 visual defect in the V22 matrix. It inspected 194 images, including eight authenticated Personal account/menu captures and eight loader frames. Two remaining P3 fixes are in source: Edit model quotation marks and the redundant Add secret tooltip.

Their fresh captures and final repository checks remain open at this report's preparation.

A green tick below means the named repair has reviewed source and V22 visual evidence.
It does not mean Indy has approved the revised gallery or that every repository gate has passed.

## Reference and comparison method

- Approved reference: unchanged `proposal.html`, SHA-256 `8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a`.
- Review inputs: Claude's first-cut report and final report. The final report replaces its earlier findings.
- Desktop: 1480 × 1000, device scale 1, fonts loaded, dark and light appearances.
- Phone: 390 × 844. Additional 696px and 900px captures check intermediate layouts.
- A 390 × 500 focused composer tests reduced viewport fit. It does not prove a physical phone keyboard's behavior.
- Screenshots use production components with labelled sample data. Authenticated account captures use development fixture accounts.
- Hover, keyboard focus, dialog autofocus, scrolled actions and top/latest transcript positions have distinct captures.
- Loader evidence includes four WebM recordings and lossless frame pairs: desktop/phone, motion allowed/reduced.
- Every package must include the same source-tree digest. Separate desktop/phone packages must not mix older revisions.
- Screenshots remain unmodified. Sample text, actual backend fields and review chrome are disclosed separately from visual differences.

The review Page holds downloadable comparisons and the original proposal: https://chatgpt.com/space/page_2523a232bea081919750d090aaba00e3.
It labels earlier gallery inputs and current delivery status separately.

## Claude final review: shared components

| Result | Finding | Implemented repair | Evidence |
|---|---|---|---|
| ✅ | Dialog title/close alignment | Shared 40px title row and aligned 40px close control; approved description spacing | All dialog families, both themes and widths; `design-system/Dialog.tsx` |
| ✅ | Green successful tool glyphs | Secondary gray action glyphs; failure/interruption keep their distinct outcomes | Activity dark/light; `FleetToolCallHeader.tsx`, `FleetToolCalls.tsx` |
| ✅ | Replace secret missing focus ring | Capture retains actual autofocus and waits for the exact replacement dialog; native outline remains | Four replacement captures and computed focus-visible/ring metrics |
| ✅ | Phone select text | Ordinary phone/coarse-pointer form selects use 16px; composer metadata keeps its separate role | Model creation and Usage computed styles; `app/globals.css` |
| ✅ | Input text too close to border | Shared Input and Select use 12px horizontal padding | Dialog field metrics; `Input.tsx`, `Select.tsx` |
| ✅ | Wake-up strip color and icon | Primary text and PencilSimple glyph | Wake-up top/latest, both themes; `WakeupStrip.tsx` |
| ✅ | Running command proportional font | Command targets use the shared monospace role | Working desktop/phone; `FleetToolCallHeader.tsx` |
| ✅ | Inconsistent recovery links | Muted Run details treatment; Refresh sessions is a secondary button | Activity/Wake-up/Security error; `FleetRunDetails.tsx`, `SessionSettings.tsx` |
| ✅ | Heavy field labels | Shared Label uses weight 500 | Invite/model/secret dialogs; `Label.tsx` |
| ✅ | Intermediate resource gutters | Removed the reserved scrollbar gutter that shifted both edges by 6px | Models/Security 696/900; `globals.css` |
| ✅ | Assistant heading size | Semantic 19px/24.7px role; registered correctly with utility merging | Measured heading y477, height24.6875; `FleetMarkdown.tsx`, shared tokens |
| ✅ | Phone status dot and intermediate cost | Phone status is hidden as a whole; cost waits for the approved wider layout | Conversation 390/696; `FleetHeaderStatus.tsx`, `FleetRunCost.tsx` |
| ✅ | Redundant Create model tooltip | Plain shared Button retains its full label | Primary-hover capture; `ModelsRegistryTable.tsx` |
| ✅ | Small type/control differences | Primary border geometry, 13px asides, 14px descriptions, 27px count chip, composer and proportional figures | Measured DOM and multi-width captures; shared Button/type roles |
| ◻ | Named dialog quotation marks | Curly quotes now match across Edit model and Replace/Delete secret; bordered Cancel retained | Source fixed; targeted Edit model recapture pending |
| ◻ | Redundant Add secret tooltip | Trigger now uses shared Button; form help remains available | Source fixed; targeted phone scrolled-action recapture pending |

## Claude final review: page composition

| Result | Finding | Implemented repair | Evidence |
|---|---|---|---|
| ✅ | Make active looked like a radio | Square native checkbox; native key-mode radio choices remain separate | Both model key modes at both widths/themes; `Checkbox.tsx` |
| ✅ | Activity lacked Changes flap | Shared recorded-write projection excludes pending calls before deduplication and counts unique resulting paths | Activity/Changes dialog; `ChangesStrip.tsx`, `WorkContextSections.tsx` |
| ✅ | Team lacked You and vertical rhythm | Fixture identity matches signed-in owner; scope/search/section spacing restored | Members heading y424/h20, panel y456; `TeamTable.tsx`, `MembersView.tsx` |
| ✅ | Phone Usage filter alignment | Right-aligned controls use the available width | Usage phone dark/light; `BillingFilters.tsx` |
| ✅ | Phone wake-up label wrapped | UTC time is compact today; other dates retain their date; midnight refresh updates the label | Wake-up phone and midnight regression; `WorkWakeups.tsx` |
| ✅ | Plugin description lost its line break | Two-line description restores section position | Plugins desktop; `library/copy.ts` |
| ✅ | Solid disclosure triangle | Native Phosphor caret in action details and named fields | Summary/secret dialogs; `ApprovalActions.tsx`, `AddSecretForm.tsx` |
| ✅ | Conversation vertical drift | Restored user/reply/tool/heading spacing | Bubble y183, assistant y341, text y371, Worked outer y411, heading y477 |
| ✅ | Schedule event copy | Scheduled wake-up wording and Next wake-up label | Runtime/browser schedule assertions; `schedule-fire.ts` |
| ✅ | Repeated transport-loss sentence | Specific recorded message appears once; generic fallback remains | Error desktop/phone; `fleetFailureCopy.tsx` |
| ✅ | New fleet glyph and phone action | TerminalWindow starter glyph and right-aligned action | New fleet both widths/themes |
| ✅ | Models card and header layout | 14px explanation, centered gray icon, stacked phone icon, heading action wraps at 696 | Models 1480/900/696/390; `ModelsRegistryView.tsx` |
| ✅ | Picker toggle in read-only Details | Read controls keep paging, retry and explicit evidence loading without the picker filter | Model Details and 55-test focused recovery run |
| ✅ | Catalogue Source/rate alignment | Source has no extra inset; wrapped values remain right-aligned | Model Details/Edit model; `DescriptionList.tsx`, `ModelPriceSchedule.tsx` |
| ✅ | Plugin option size/gap | 44px options with 8px gap | Plugin dialog desktop/phone |
| ✅ | Invite scope icon | Users icon and approved account-scope copy | Invite desktop/phone; `InviteDialog.tsx` |
| ✅ | Duplicated secret notice | Header supplies the notice once; standalone/replacement forms keep the default | Add secret captures and tests |
| ✅ | Replacement naming/mobile fields | Replace secret / Replace value agree; masked value spans the phone row | Replacement dark/light and tests |
| ✅ | App dialog action order | Done is primary/rightmost; Disconnect is secondary | Connected app dialog both widths/themes |
| ✅ | Missing phone plugin label | Published setup remains beside its menu | Plugins phone dark/light |
| ✅ | Session row rhythm | Shared tile, text spacing and minimum height restored | Combined Security both themes; `SessionSettings.tsx` |
| ✅ | Usage row/caption treatment | Real line spacing yields 77px rows; Daily spend caption is quieter | Usage desktop dark/light; `BillingReportView.tsx` |
| ✅ | Secrets/app panels too low | Shared 24px page section gap | Secrets/Connected apps desktop dark/light |

The original Usage CSS left-aligns Activities and right-aligns only Charged.
The review retains that alignment; it does not apply the final report's mistaken Activities suggestion.

## Evidence repairs

| Result | Earlier gap | Current evidence |
|---|---|---|
| ✅ | Static-looking compressed loader | Lossless pairs show 562 changed desktop pixels and 553 changed phone pixels, maximum channel difference 49 |
| ✅ | Missing phone animation recording | Separate 1480px and 390px recordings, each with motion allowed and reduced |
| ✅ | Reduced-motion uncertainty | Both reduced-motion frame pairs have exactly zero changed pixels |
| ✅ | Missing lifecycle states | Archive, Permanently Delete and resulting archived/deleted notices captured in both themes and widths |
| ✅ | Missing account surfaces | Authenticated native Personal account and account menu captured at both widths and themes |
| ✅ | Mixed desktop/phone source | One V22 capture tree supplies both packages; final regenerated packages must retain that rule |
| ✅ | Duplicate screenshots offered as coverage | Main matrix has 178 distinct state images and no duplicate hashes |
| ✅ | Invisible field focus during capture | Capture no longer blurs dialog inputs; exact-dialog waits preserve focus evidence |
| ✅ | Light theme/menu disagreement | Capture sets saved theme before ThemeProvider navigation; account menus now report the displayed theme |

## Functional repairs retained

- ✅ Model credentials use the explicit workspace already backed by the API. No primary-vault selection or key copying exists.
- ✅ Model creation keeps saved credentials after registration failure, keeps registered models after activation failure and reconciles uncertain responses before retry.
- ✅ Read-only catalogue details retain continuation, retry, precise rates and selected model identity.
- ✅ Native secret-dialog handoff restores the lasting Manage trigger before opening replacement/deletion. Removing the row cancels the queued action.
- ✅ API-key one-time receipt handling survives overlapping React Actions without exposing a dismissed receipt again.
- ✅ Session discovery follows all pages, retains the current device and separates partial/read failures from confirmed sign-out.
- ✅ Wake-up actions retain scoped ownership, cancellation, deduplication and uncertain-response recovery.
- ✅ Plugin publication requires an explicit owned destination. Selecting a destination does not write.
- ✅ Confirmed saved identity drives setup-to-fleet navigation; setup history and runtime instructions retain separate owners.
- ✅ Direct Permanently Delete performs the required stop internally. Partial failure retains archived history and retries only deletion.
- ✅ Inline Approve/Deny retains existing permission and decision checks. Separate Approvals/Grants destinations are removed.
- ✅ Upload stays an authorized preview no-op. Its receipt states that bytes were not stored or analyzed.
- ✅ Native assistant-ui owns messages, tool rendering, attachments, actions, reasoning and composer behavior.
- ✅ Auth-provider details remain behind the facade. Personal account uses native provider controls; Security uses scoped session operations.

These green functional entries record the reviewed implementation and focused regression evidence.
The fresh full browser/authenticated lanes below decide current candidate readiness.

## Actual data differences and next work

| Area | Current UI behavior | Post-merge backend work |
|---|---|---|
| Usage | Settled totals and full charge details; Daily spend explicitly unavailable | UTC daily buckets and complete per-fleet model facts |
| API keys | Actual creation/revocation dates; no invented suffix or last-use time | Add safe suffix/last-used metadata if recorded |
| Secrets | Hidden values; Replace/Delete; access details unavailable | Project existing access policy; a new access-control model needs separate design |
| Plugins | Published setup label; immutable revisions inspected in dialog | Return revision ordinal metadata where the API currently omits it |
| Changes/Outputs | Only recorded writes and confirmed publication evidence | Recorded diff totals/publication receipts where absent; no made-up totals |
| Composer | Actual selected model; neutral empty Send; supported preview uploads only | Runtime context-budget facts if already recorded; real uploads/mentions remain separate scope |
| Account | Native Personal account; combined API keys and sessions | No extra custom Profile/API-key page |

Source review confirms the current API-key database has no display suffix or last-use field.
Usage already has complete period-level fleet and model groups; the missing projection relates each fleet to its complete model set.

Secret execution-policy declarations exist, but SecretSummary exposes no effective access projection.
Plugin detail and history already expose decimal revision numbers; the collection card omits them.

Usage More filters preserves real model filtering without crowding the approved primary toolbar. Delete Plugin preserves the existing supported operation inside its dialog. The sample uses a November period consistent with its November reference clock.

Picker dialogs retain advanced-model filtering; the read-only Details dialog does not.

Indy authorized backend completion after the UI merge:

The UI spec records the owner’s exact approval under Approved parallel CLI/API follow-up.

The combined CLI/API and scoped-discovery follow-up is [M249_002](../pending/M249_002_P1_API_CLI_DOCS_UI_FLEET_COMMANDS_AND_SCOPED_DISCOVERY.md).
It may start independently now, but merges after the UI PR and rebases before refreshed caller changes.
Flow 2 capabilities, real upload storage, mention invocation, new secret policy and a platform UI are excluded.

## Post-merge implementation start

Use these instructions if the downloadable backend-gap prompt cannot be opened.

1. Verify the UI refresh PR has merged. Fetch main and create a separate worktree from that merge.
2. Read repository instructions, the design system and this report. Keep the running CLI worktree separate.
3. Trace each data gap to current response schemas, stored facts, permission checks and UI adapters. Record exact files and fields.
4. Classify each gap: already available, adapter-only, backend change or unresolved design. Do not assume every gap needs an endpoint.
5. Create and review a pending implementation spec before editing. Preserve existing authority, native chat owners and the approved layouts.
6. Implement only supported recorded data. Keep unavailable states for missing facts; do not invent percentages, access grants or publication outcomes.
7. Coordinate shared API names with the CLI agent. Run required checks, capture changed visuals and open a separate PR without merging.

## UI PR completion punchlist

These items remain in the UI refresh and must not be transferred to the backend follow-up.

- [ ] Complete current lint, coverage and rendered-page checks after the workspace restart.
- [ ] Complete real authenticated resource flows, `make acceptance-codex` and the required repository checks.
- [ ] Build the app and website, then run their existing bundle-size checks. Report any excess for owner sign-off.
- [ ] Recapture the two post-V22 polish fixes and compare any new visual changes.
- [ ] Resolve Claude's new review findings against the actual candidate revision.
- [ ] Complete source review, orphan sweep, documentation and spec evidence.
- [ ] Commit, push, open the UI PR and resolve CI and Greptile. Do not merge.

The post-merge Claude prompt covers only the data inventory above.
Its first step requires a fresh source inventory and reviewed spec before implementation.

## Verification ledger

| Check | Recorded result | Current candidate status |
|---|---|---|
| Independent visual audit | 194 images inspected; no open P2; two P3 fixes applied afterward | Targeted recapture pending |
| Focused catalogue/secret recovery | 55 tests passed across five files | Passed before latest main merge |
| Focused shared merge/Button behavior | 32 tests passed across two files | Passed before latest main merge |
| Native authenticated Personal account/menu | Two browser journeys passed, 27.4s; real development provider | Passed before latest main merge |
| Final UI lint/coverage/dry | `make lint-app lint-design-system test-coverage-all dry-app` | Running on merged candidate |
| Required repository checks | Declared lint, unit, integration, version and PR gate | Pending |
| Current authenticated resource journeys | Real development API/provider, isolated fresh task database | Pending rerun |
| Owner eyeball approval | Indy requested further corrections | Not claimed |
| UI implementation PR | Not open at preparation | Pending |

The branch includes main `4f3159269644b2a1d814fbcc9119f01830b7eaf8` through merge `aa0882080`.
The previous base integration timeout occurred on isolated base `e8a5e8022`; it is not attributed to this UI change.
Final required checks must report their actual results, including failures, before delivery claims change.


## Current candidate performance

| Check | Measured result | Disposition |
|---|---|---|
| App shared dashboard | 292.30 kB / 294.91 kB | Pass; only this cap increased with owner approval |
| New fleet incremental | 87.38 kB / 102.40 kB | Pass |
| Saved fleet incremental | 98.86 kB / 102.40 kB | Pass |
| Other app routes | Existing limits | Pass |
| Website JavaScript | 109.71 kB / 128 kB | Pass |
| Website CSS | 19.17 kB / 20 kB | Pass |

App evidence: `refresh-app-bundle-size-v113.log`; website evidence: `refresh-website-bundle-size-v104.log`.
Indy approved only the shared dashboard cap from 270 KiB to 288 KiB:
"Approve shared cap of 288 KiB". He also requested further reductions where possible.
Every route and website limit remains unchanged. The size check must pass against the approved cap.

The additional bounded source review found no concrete defect in the conditional native setup/lifecycle boundaries and no further clear low-risk eager-shell reduction. Existing menu/history/analytics boundaries are already deferred. The reverted mobile-dialog experiment shifted bytes into the constrained saved-fleet route; it is not part of the delivered source. The approved cap passed in `refresh-app-bundle-size-v113.log`.
