/*
 * Shared underline tab visual — the ONE tab style (pill retired).
 *
 * Both the in-page Radix `Tabs` (active marked via `data-state=active`) and the
 * route-style `TabNav` links (active marked via `data-active=true`) compose
 * these constants, so the underline visual is defined exactly once (RULE UFS —
 * imported verbatim by the components AND their tests). The active-state
 * SELECTOR is the only thing that differs between the two, so the trigger class
 * is a shared base + a per-primitive active suffix.
 *
 * Active indicator = a `--pulse` underline (a sanctioned "active" use of the
 * currency accent per docs/DESIGN_SYSTEM.md) over a thin `--border` rail. No
 * rounded pill tray, no `bg-background` active fill, no shadow. Visual source
 * of truth: docs/DESIGN_SYSTEM.md (Component principles → Tabs).
 */

/** Underline rail: a thin bottom border the triggers sit on — replaces the
 *  retired pill tray (a muted-fill rounded container). */
export const TAB_LIST_CLASS =
  "flex items-center justify-start gap-1 border-b border-border text-muted-foreground";

/** Shared trigger chrome — everything except the active-state selector. The
 *  `-mb-px` laps the trigger's 2px underline over the rail's 1px border. */
const TAB_TRIGGER_BASE =
  "inline-flex items-center justify-center whitespace-nowrap px-3.5 py-2.5 -mb-px " +
  "text-body-sm font-medium no-underline border-b-2 border-transparent " +
  "ring-offset-background transition-colors hover:text-foreground " +
  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 " +
  "disabled:pointer-events-none disabled:opacity-50";

/** Active underline for the Radix `Tabs` trigger (sets `data-state=active`). */
export const TAB_TRIGGER_CLASS_RADIX = `${TAB_TRIGGER_BASE} data-[state=active]:border-pulse data-[state=active]:text-foreground`;

/** Active underline for the route-link `TabNav` item (sets `data-active=true`). */
export const TAB_TRIGGER_CLASS_LINK = `${TAB_TRIGGER_BASE} data-[active=true]:border-pulse data-[active=true]:text-foreground`;
