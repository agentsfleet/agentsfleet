# M249 rendered design review

Reviewed November 15, 2026, against the owner's [ux.html](../../ux.html) and [design-system rules](../../DESIGN_SYSTEM.md).
This review covers the dashboard pages retained from PR #844 and the visual changes in PR #845.
The second independent reviewer was GPT-6 Astra, as the owner requested.

## Reference and method

The reference retains SHA-256 `b3ba83c2de2292e7f65193c85d97d299ec4d01a7484c7243fb9a5d42da5d52fe`.
The review uses the owner's later instructions when they differ from its sample screens.
No sample record establishes a real charge, permission, session, fleet description or model identity.

The comparison covers 13 pages and states at 1480×1000, 768×1024 and 390×844 in both themes.
Each state has a reference screenshot, a production-component screenshot and browser measurements.
The main comparison contains 78 pairs, or 156 captures, with no recorded page errors or document overflow.

Both sides hide only their sample review toolbars to align the outer frame.
Additional phone captures scroll to the top for setup, conversation, activity, error and wake-up.
The original captures retain the application's initial focus and latest-message scrolling.

The components use task-owned local data and simulated HTTP responses.
These captures do not establish authenticated acceptance, live provider totals or successful server writes.
The [runtime record](M249_POST_MERGE_RUNTIME_VALIDATION.md) keeps those results separate.

Task files retain the original captures under `design-audit`, corrected captures under `design-audit-final`, and dialog captures under `design-audit-interactions`.
The later help repair has its own `design-audit-policy-final` resting-state captures.
The independent report is retained as `astra-design-review.md` with its focused follow-up.

## Typography and chart implementation

The application and reference render Manrope.
The family aliases differ, but the tested text has identical glyph widths and canvas pixels at weights 400, 500, 600, 650 and 750.
The comparison checks rendered text rather than inferring font loading from a CSS family name.

Page titles use 20px/26px at weight 650.
Ordinary desktop reading uses 13px/20px; phone conversation reading and typing retain the approved 16px roles.
Technical values retain the shared monospace role.

[BillingDailySpend](../../../ui/packages/app/app/(dashboard)/settings/billing/components/BillingDailySpend.tsx) renders ordinary CSS bars inside the shared `DashboardPanel`.
It adds no chart library, canvas renderer or chart-specific dependency.
The same report supplies totals, daily bars and fleet spending.

| Measurement | Reference | Corrected application |
|---|---|---|
| Chart outer border and corners | 1px; 16px | 1px; 16px |
| Desktop chart | 1040×206 | 1040×206 |
| Tablet chart | 638×206 | 638×206 |
| Phone chart | 296×198 | 296×198 |
| Caption-to-plot gap | 16px | 16px |
| Plot height | 100px | 100px |
| Bar top corners | 4px | 4px |
| Maximum bar headroom | 90 percent of plot | 90 percent of plot |
| Bar tint | Primary mint; ordinary bars at 80 percent | Shared primary token; ordinary bars at 80 percent |
| Resource inset at 768px | 24px | 24px |
| Team phone member panel | y532; 296×259 | y532; 296×259 |

The 100px measurement describes the plot area, not a guaranteed visible charge.
Zero charges keep zero-height bars and an explicit empty message.
The decoder still refuses more than 31 daily buckets; fleet count does not increase the number of daily bars.

## Corrected findings

| Finding | Before | Repair and verification |
|---|---|---|
| Chart geometry | 210px panel; 24px caption gap; 20px phone vertical padding | 206px desktop and 198px phone; 16px gap and phone padding, measured in both themes |
| Bar treatment | Success tint, 70 percent ordinary bars, 2px corners, full-height maximum | Primary mint, 80 percent ordinary bars, 4px corners and reference headroom |
| Totals baseline | Block amount glyphs sat 3px lower | Inline summary amount matches the reference baseline and panel height |
| Intermediate canvas | 32px inset at 768px | Shared 768–800px rule restores 24px inset without changing wider layouts |
| Page and section actions | Early side-by-side headers; small phone gaps | Shared headers stack through 800px; page and section gaps match the reference |
| Team scope | Extra phone top padding, wide gap and centered icon | Top alignment, zero top padding, 12px gap and aligned owner badge |
| Team counts | 20×20 badges and extra heading height | 25×27 badges with the reference heading and panel rhythm |
| Usage controls | Mismatched gaps and foreground treatment | Shared secondary text, responsive gaps and bounded control widths |
| Package-manager help | Touch activation exposed no explanation | Existing shared dialog opens from the information control; policy state stays unchanged |

The help repair keeps desktop hover text and changes no other action tooltip.
Its four focused tests cover activation, focus return, disabled settings, unknown policy and retention of a selected disabled option.
Before the repair, two activation tests failed because no dialog appeared; the unknown-policy test passed.

## Page-by-page result

Every row includes desktop, tablet and phone in dark and light.

| Page or state | Reviewed result | Necessary differences from sample |
|---|---|---|
| Usage | Typography, borders, chart, totals, controls and fleet table reviewed; chart geometry corrected | Exact All Fleets; removed fixed-zone/sample caption; truthful local dates and model labels; paused secondary actions remain absent |
| Security | Shield, title, grouped keys and sessions, creation dialog, avatar and account menu reviewed | No repeated identity card; real key prefixes, date wrapping and unavailable metadata remain truthful |
| Team | Scope, search, member rows, invitation outcomes, count badges and invite dialog reviewed | The viewer uses application identity; joined dates and invitation outcomes come from supplied records |
| Models | Default card, saved models, creation dialog and responsive action placement reviewed | Owner-approved shorter default description; actual provider/model names and retained management actions |
| Connected apps | Heading, resource rows, boundaries, phone stacking and actions reviewed | No invented expired deployment connection or fabricated repository count |
| Secrets | Header, table, bounded phone scrolling and add dialog reviewed | Access remains unavailable when not reported; secret values stay hidden |
| New fleet | Brand mark, heading, composer, starters, disabled send and focus boundary reviewed | Plugin starter remains absent; empty input keeps creation disabled |
| Fleet setup | Header, Summary, policy controls, save action, composer and scheduler recovery reviewed | Approved 320px setup Summary; no invented Plugin provenance; help activation repaired |
| Conversation | Reading scale, attachment, bubble, work disclosure, output, Summary and composer reviewed | Recorded Sources and Tasks remain; no fabricated change counts or notifications |
| Activity | Shared transcript, disclosure and Summary geometry reviewed | Different recorded actions limit exact row-for-row comparison with the sample |
| Error | Semantic error surface, retry, diagnostics and retained conversation reviewed | Interrupted request differs from the sample's expired connection; no invented reconnect action |
| Wake-up | Local times, recorded schedule activity, composer flap and editor reviewed | Current Summary uses recorded Sources and Tasks; no duplicate setup wake-up row |
| Empty saved conversation | Saved header, inline Summary, empty facts and composer reviewed | No exact reference state exists; New fleet is a different product state |

The [Plugin pause](../parked/README.md) remains in force.
The reference's Plugin navigation, source routes, starter and publication examples do not activate those features.
Their reusable components remain available for later work.

The reference contains no authentication-provider screens.
This review cannot establish their pixel parity; earlier authentication acceptance remains recorded separately.

## CSS and loading cost

The geometry corrections change class strings in eight existing components and one bounded media rule.
They add no JavaScript breakpoint handling, per-bar measurement, observer, timer, request or animation.
Shared page, section and collection headers own the repeated spacing rules.

The information disclosure reuses the existing dialog and tooltip primitives.
It opens locally and changes neither network policy nor report loading.
The shared model-label cache remains the only Usage catalogue source.

The one- and 100-fleet browser checks remain the source for report counts, menu bounds and chart bounds.
Collection results and injected-report timings are not live production performance measurements.

| Browser fixture, both themes | Report reads per flow | Extra catalogue or charge reads | Populated October chart | First report bytes |
|---|---|---|---|---|
| Desktop, one fleet plus Setup | 3 | 0 | 31 daily buckets; 100px plot | 7,415 |
| Desktop, 100 fleets plus Setup | 3 | 0 | 31 daily buckets; 100px plot | 54,130 |
| Phone, one fleet plus Setup | 3 | 0 | 31 daily buckets; 100px plot | 7,415 |
| Phone, 100 fleets plus Setup | 3 | 0 | 31 daily buckets; 100px plot | 54,130 |

Each flow loads once, selects the last fleet, and restores All Fleets.
Each action makes one report request; Setup returns with All Fleets.
The menu remains within the viewport, including the 101-option case.

An injected 1,200ms label delay holds only the named fleet table.
The same one-report and zero-extra-read assertions pass while totals and chart remain available.
These response sizes describe the current report shape, before the announced API cleanup.

## Review limits and completion

The reviewer found one remaining P2 after the first corrections: package-manager help lacked touch activation.
The focused Astra follow-up confirms that repair across eight contexts and 24 interaction sequences.
Those contexts cover desktop and phone, both themes, and restricted or public internet with package domains selected.

Click or tap, Enter and Space open help; Done, Escape and Close return focus without writes or policy changes.
This review does not claim universal pixel equality across different records, errors or unprovided reference states.

The review does not prove live mutations, provider billing truth, browser zoom, translations or every loading permutation.
The pending parent API cleanup requires a separate compatibility check when its commit arrives.
Rust coverage and PR merge readiness remain separate from this design result.

The focused Astra follow-up reports no remaining actionable design finding.
Thirty additional boundary checks confirm the actual viewport widths: 320, 360, 640, 680, 681, 800, 801, 1100, 1101 and 1920px.
Usage, Models and Security have no document overflow or out-of-bounds combobox in those checks.

The first native dry run failed because inherited environment-file paths do not exist.
The retry clears those three file pointers and retains the configured process values.
The first lint run also identified two test query options unsupported by Testing Library; those options were removed.

The corrected `make lint-app lint-design-system` passes, including 44 source tests and both packages' type checks.
The corrected `CI=true E2E_PORT=3191 make dry-app` passes 4,695 unit tests across 493 files and 134 browser tests.
The browser report records zero failed, flaky or skipped tests.

The standards review requested the existing named breakpoint utility instead of an equivalent numeric utility.
That final class-only cleanup uses `summary` and `summary-medium`; a focused standards follow-up reports no findings.

The subsequent app production build and every authenticated route size budget pass.
Thirty width checks repeated on the shared aliases confirm no document overflow or clipped combobox.

| Production JavaScript, gzipped | Measured size | Existing limit |
|---|---|---|
| Shared authenticated dashboard | 290.57 kB | 294.91 kB |
| Usage route addition | 34.08 kB | 102.4 kB |
| Security route addition | 41.27 kB | 102.4 kB |

The shared dashboard size remains unchanged from the earlier reviewed checkpoint.
These bundle measurements and the browser request counters support the loading assessment; they do not establish production latency.


## Recheck after the report update

The owner supplied `6edd08d84`, including the daily rollup and trimmed Usage report.
PR #845 was rebased onto that revision while retaining its reviewed visuals.
The [cleanup record](M249_UI_CLEANUP.md) separates the inherited API work from the UI removals.

A second complete capture contains 156 states with zero page errors and zero document overflow.
Every recorded font probe matches the reference; recorded geometry matches the earlier corrected matrix.
Astra also opened model selection on desktop and phone without a model-name regression.

The earlier performance table describes the previous injected response shape.
The trimmed-report [cleanup record](M249_UI_CLEANUP.md) contains the new response sizes and current browser timings.
The final dry lane passes 4,575 unit tests and 134 browser tests; Usage adds 33.95 kB gzipped after the successful production build.
