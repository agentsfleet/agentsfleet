import { type ElementType } from "react";

/*
 * TabNav — route-style tab bar: a <nav> of links styled as pills. Unlike Tabs
 * (a Radix in-page tablist that swaps panels), these are navigation between
 * destinations, so each is a real link with aria-current.
 *
 * Framework-agnostic on purpose: the design-system ships to a Vite site and a
 * Next app, so it must not import next/*. The consumer injects its router link
 * via `linkComponent` (e.g. Next <Link>) and computes `activeHref`.
 *
 *   <TabNav
 *     label="Settings sections"
 *     items={[{ label: "Basic Info", href: "/settings" }]}
 *     activeHref={pathname}
 *     linkComponent={NextLink}
 *     onNavigate={(href) => track(href)}
 *   />
 */
export type TabNavItem = { label: string; href: string };

export type TabNavProps = {
  items: TabNavItem[];
  /** The href of the currently-active tab (consumer-computed). */
  activeHref: string;
  /** Accessible name for the nav landmark. */
  label: string;
  /** Router link component (Next <Link>, etc.). Defaults to a plain anchor. */
  linkComponent?: ElementType;
  /** Fired with the item href on click — for analytics, etc. */
  onNavigate?: (href: string) => void;
};

const TAB_CLASS =
  "inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1.5 text-sm font-medium no-underline " +
  "ring-offset-background transition-all hover:text-foreground " +
  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 " +
  "data-[active=true]:bg-background data-[active=true]:text-foreground data-[active=true]:shadow-sm";

export function TabNav({ items, activeHref, label, linkComponent, onNavigate }: TabNavProps) {
  const LinkEl: ElementType = linkComponent ?? "a";
  return (
    <nav
      aria-label={label}
      className="inline-flex h-10 max-w-full items-center justify-start gap-1 overflow-x-auto rounded-lg bg-muted p-1 text-muted-foreground"
    >
      {items.map((item) => {
        const active = item.href === activeHref;
        return (
          <LinkEl
            key={item.href}
            href={item.href}
            aria-current={active ? "page" : undefined}
            data-active={active ? "true" : undefined}
            className={TAB_CLASS}
            onClick={onNavigate ? () => onNavigate(item.href) : undefined}
          >
            {item.label}
          </LinkEl>
        );
      })}
    </nav>
  );
}

export default TabNav;
