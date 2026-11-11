// The underline rail keeps the active trigger aligned with its neighbours.
export const TAB_LIST_CLASS =
  "flex items-center justify-start gap-2xl border-b border-border text-muted-foreground";

/** Shared trigger chrome — everything except the active-state selector. The
 *  `-mb-px` laps the trigger's 2px underline over the rail's 1px border. */
const TAB_TRIGGER_BASE =
  "tab-trigger relative inline-flex min-h-11 items-center justify-center whitespace-nowrap px-0 py-2.5 -mb-px " +
  "font-sans text-body-sm font-normal text-muted-foreground no-underline border-b-2 border-transparent " +
  "ring-offset-background transition-colors duration-fade ease-fade hover:text-foreground " +
  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 " +
  "disabled:pointer-events-none disabled:opacity-50";

/** Active underline for the Radix `Tabs` trigger (sets `data-state=active`). */
export const TAB_TRIGGER_CLASS_RADIX = `${TAB_TRIGGER_BASE} data-[state=active]:text-foreground data-[state=active]:font-medium`;
