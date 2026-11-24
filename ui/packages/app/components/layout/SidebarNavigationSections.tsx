"use client";

import type { ReactNode } from "react";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import Link from "next/link";
import { cn, NavItem as NavigationItem, Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@agentsfleet/design-system";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { NAV_SURFACE, navSource, resolveHref, type NavEntry } from "./sidebar-navigation-data";

export type NavListProps = {
  activeHref: string;
  workspaceId: string | null;
  onNavigate: () => void;
  collapsed: boolean;
  overlay?: boolean;
};

export function NavSection({ items, ...props }: NavListProps & { items: NavEntry[] }) {
  return (
    <NavGroup>
      <NavItems items={items} {...props} />
    </NavGroup>
  );
}

function NavItems({ items, activeHref, workspaceId, onNavigate, collapsed, overlay }: NavListProps & { items: NavEntry[] }) {
  return items.map((entry) => {
    const href = resolveHref(entry, workspaceId);
    return (
      <NavItem
        key={entry.label}
        href={href}
        label={entry.label}
        Icon={entry.icon}
        create={entry.path === "fleets/new"}
        external={entry.external}
        active={!entry.external && href !== "" && href === activeHref}
        collapsed={collapsed}
        overlay={overlay}
        onClick={() => {
          onNavigate();
          trackNavigationClicked({ source: navSource(entry), surface: NAV_SURFACE, target: href });
        }}
      />
    );
  });
}

function NavGroup({ children }: { children: ReactNode }) {
  return (
    <TooltipProvider><div className="flex flex-col items-center gap-md px-sm md:items-stretch md:px-lg">
      {children}
    </div></TooltipProvider>
  );
}

function NavItem({
  href,
  label,
  Icon,
  create,
  active,
  external,
  collapsed,
  overlay,
  onClick,
}: {
  href: string;
  label: string;
  Icon: PhosphorIcon;
  create: boolean;
  active?: boolean;
  external?: boolean;
  collapsed?: boolean;
  overlay?: boolean;
  onClick: () => void;
}) {
  const content = (
    <>
      <Icon size={20} weight={active ? "bold" : "regular"} className="shrink-0" aria-hidden="true" />
      {collapsed ? null : <span className={overlay ? undefined : "hidden md:inline"}>{label}</span>}
    </>
  );
  const common = {
    "aria-label": label,
    appearance: "rail" as const,
    tone: create ? "accent" as const : undefined,
    className: cn(!overlay && "w-11 justify-center px-0 md:w-full", !collapsed && "md:justify-start md:px-lg", create && "mb-md"),
    onClick,
  };
  const item = external
    ? <NavigationItem href={href} target="_blank" rel="noopener noreferrer" {...common}>{content}</NavigationItem>
    : <NavigationItem asChild active={active} {...common}><Link href={href}>{content}</Link></NavigationItem>;
  return collapsed
    ? <Tooltip><TooltipTrigger asChild>{item}</TooltipTrigger><TooltipContent side="right">{label}</TooltipContent></Tooltip>
    : item;
}
