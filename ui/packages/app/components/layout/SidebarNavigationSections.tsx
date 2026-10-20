"use client";

import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import Link from "next/link";
import { Button, cn, NavItem as NavigationItem } from "@agentsfleet/design-system";
import { useFreshFleetChat } from "./draft-sidebar-state";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { NAV_SURFACE, NAV_ICON_SIZE, NAV_ICON_STROKE_WIDTH, navSource, resolveHref, type NavEntry } from "./sidebar-navigation-data";

export type NavListProps = {
  activeHref: string;
  workspaceId: string | null;
  onNavigate: () => void;
  collapsed: boolean;
};

export function NavSection({ items, ...props }: NavListProps & { items: NavEntry[] }) {
  return (
    <NavGroup>
      <NavItems items={items} {...props} />
    </NavGroup>
  );
}

function NavItems({ items, activeHref, workspaceId, onNavigate, collapsed }: NavListProps & { items: NavEntry[] }) {
  const fresh = useFreshFleetChat(workspaceId);
  return items.map((entry) => {
    const href = resolveHref(entry, workspaceId);
    return (
      <NavItem
        key={entry.label}
        href={href}
        label={entry.label}
        Icon={entry.icon}
        external={entry.external}
        active={!entry.external && href !== "" && href === activeHref}
        collapsed={collapsed}
        onClick={() => {
          if (entry.path === "fleets/new") fresh();
          onNavigate();
          trackNavigationClicked({ source: navSource(entry), surface: NAV_SURFACE, target: href });
        }}
      />
    );
  });
}

function NavGroup({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col gap-xs px-sm">
      {children}
    </div>
  );
}

function NavItem({
  href,
  label,
  Icon,
  active,
  external,
  collapsed,
  onClick,
}: {
  href: string;
  label: string;
  Icon: LucideIcon;
  active?: boolean;
  external?: boolean;
  collapsed?: boolean;
  onClick: () => void;
}) {
  const content = (
    <>
      <Icon size={NAV_ICON_SIZE} strokeWidth={NAV_ICON_STROKE_WIDTH} className="shrink-0" aria-hidden="true" />
      {collapsed ? null : label}
    </>
  );
  const common = {
    title: collapsed ? label : undefined,
    "aria-label": collapsed ? label : undefined,
    className: cn(collapsed && "justify-center px-0"),
    onClick,
  };
  if (label === "New fleet") return <Button asChild variant="secondary" className={cn("w-full", collapsed ? "px-0" : "justify-start")}><Link href={href} {...common}>{content}</Link></Button>;
  if (external) {
    return <NavigationItem href={href} target="_blank" rel="noopener noreferrer" {...common}>{content}</NavigationItem>;
  }
  return <NavigationItem asChild active={active} {...common}><Link href={href}>{content}</Link></NavigationItem>;
}
