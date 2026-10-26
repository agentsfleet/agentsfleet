# Design system CSS audit

Verified October 26, 2026 against baseline `d6d279cfde5f3c6ec5eb75b088fa1d2bac1af371` and the current working changes.
This report covers `tokens.css`, `theme.css`, `interactions.css`, and their shared composer consumers.
The [design system standard](../DESIGN_SYSTEM.md#ownership-and-enforcement) owns the rules for future changes.

## Findings and changes

| Finding | Evidence | Result |
|---|---|---|
| Motion and layout selectors lived among token values | Baseline `tokens.css` contained keyframes, spinners, reduced-motion rules, and history layout skipping | Moved those selectors into `interactions.css`; token values stay in `tokens.css` |
| Press feedback had repeated selectors | Baseline declared `.control-motion:active` in separate motion blocks | One rule now follows the hover rule; pressing still overrides the hover transform |
| Reading typography and width aliases had no consumers | Search found only their definitions, theme forwards, and class-merging registrations | Removed `--fs-reading`, `--lh-reading`, `--max-width`, `--measure`, and their dependent registrations |
| Eight minimum-width forwards were unnecessary | Tailwind 4.3.3 produced identical named minimum and maximum width rules before and after removal | Removed `--min-width-*`; `--container-*` supplies both utility families |
| Warning had two utility spellings | `--color-warn` and `--color-warning` both referenced `--warn`; only `warning` utilities had consumers | Removed the unused `warn` utility spelling; palette token `--warn` remains |
| A font value bypassed the token file | `--font-system` declared its font stack directly in `theme.css` | Added `--ff-system` in `tokens.css`; the theme now forwards that token |
| Composer callers chose different typography | Fleet chat used a static model label; setup used the default select size | Shared composer input, model, and hint controls now own their typography |
| A theme comment contradicted light mode | `--on-pulse` changes in the light theme | Corrected the comment without changing the colors |

The PostCSS parser counted 408 declarations before these changes and 393 after them across the three files.
It found zero repeated property declarations within individual blocks in either version.
This count includes properties within keyframes and media queries.

## Shared values that should remain

| Shared value | Separate roles | Why the names remain |
|---|---|---|
| `--text` | Foreground, card foreground, popover foreground | These surfaces can change independently |
| `--surface-2` | Muted, secondary, popover | Equal colors do not make the surfaces interchangeable |
| `--pulse` | Primary and focus ring | A control fill and keyboard focus have separate jobs |
| `--border` | Border and input | Input boundaries can change independently |
| `--error` | Error text and destructive actions | Status and action roles can need different contrast |
| `--r-md` | Default radius and medium radius | A default and a named scale step serve different consumers |

Both files use `:root, [data-theme]` for different properties.
That repeated selector does not overwrite the same declarations.
Light-mode token overrides intentionally repeat property names beneath a different selector.

## Remaining naming and ownership issues

The text-color vocabulary still overlaps.
`text-text` matches `text-foreground`; `text-text-muted` matches `text-muted-foreground`.
Website and shared-component callers still use both families.

New interface code uses `text-foreground` and `text-muted-foreground`.
Removing the active duplicate names requires changing their callers together.
`text-text-chat`, `text-text-subtle`, and `text-text-dim` remain distinct roles.

The billing page still selects `font-system` and overrides typography on shared controls.
Examples include `BillingFilters.tsx`, `BillingSpending.tsx`, and `BillingReportView.tsx` under the app's billing route.
Those call sites need shared component variants before the local overrides can be removed.

The existing size-name collision in Tailwind also remains.
The compiler resolves generated `max-w-xl` to the custom spacing value, while the later utility rule restores the intended width.
Keep that repair until consumers and spacing names can change together.

## Enforcement and evidence

| Check | What it proves | Limit |
|---|---|---|
| PostCSS declaration audit | Repeated properties, selectors, and references across the three files | Equal values alone do not establish duplicate meaning |
| Tailwind 4.3.3 compiler comparison | All eight named minimum and maximum widths remain unchanged after forward removal | Browser checks still establish the applied cascade |
| `.orly/audits/design-tokens.sh` | Named utilities, allowed fills, and consumer font ownership | Static checks do not establish visual parity |
| Composer browser checks | Matching computed font family, size, weight, and line height | Other product surfaces need their own checks |
| Completion movement checks | Streaming completion keeps its text node and scroll position | They cover the exercised reply shapes and viewports |

The compiler comparison covered `trim`, `narrow`, `measure`, `form`, `wide`, `content`, `tagline`, and `prose`.
The press-feedback probe failed with the old rule order and passed after consolidation.
These checks supplement the shared component tests and production app build.
