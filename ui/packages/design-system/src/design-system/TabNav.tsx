import { type ElementType } from "react";
import { TAB_LIST_CLASS, TAB_TRIGGER_CLASS_LINK } from "./tab-styles";

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

export function TabNav({ items, activeHref, label, linkComponent, onNavigate }: TabNavProps) {
  const LinkEl: ElementType = linkComponent ?? "a";
  return (
    <nav
      aria-label={label}
      className={`${TAB_LIST_CLASS} max-w-full overflow-x-auto`}
    >
      {items.map((item) => {
        const active = item.href === activeHref;
        return (
          <LinkEl
            key={item.href}
            href={item.href}
            aria-current={active ? "page" : undefined}
            data-active={active ? "true" : undefined}
            className={TAB_TRIGGER_CLASS_LINK}
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
