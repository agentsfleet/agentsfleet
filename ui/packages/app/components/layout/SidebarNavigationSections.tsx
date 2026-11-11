"use client";

import type { ReactNode } from "react";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import Link from "next/link";
import { cn, NavItem as NavigationItem, Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@agentsfleet/design-system";
import { useFreshFleetChat } from "./draft-sidebar-state";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { NAV_SURFACE, navSource, resolveHref, type NavEntry } from "./sidebar-navigation-data";

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
    <TooltipProvider><div className="flex flex-col gap-md px-lg">
      {children}
    </div></TooltipProvider>
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
  Icon: PhosphorIcon;
  active?: boolean;
  external?: boolean;
  collapsed?: boolean;
  onClick: () => void;
}) {
  const content = (
    <>
      <Icon size={20} className="shrink-0" aria-hidden="true" />
      {collapsed ? null : label}
    </>
  );
  const common = {
    "aria-label": collapsed ? label : undefined,
    className: cn("min-h-12 gap-lg", collapsed ? "justify-center px-0" : "px-lg", label === "New fleet" && "bg-cta text-cta-foreground hover:bg-cta-hover hover:text-cta-foreground data-[active=true]:bg-cta data-[active=true]:text-cta-foreground"),
    onClick,
  };
  const item = external
    ? <NavigationItem href={href} target="_blank" rel="noopener noreferrer" {...common}>{content}</NavigationItem>
    : <NavigationItem asChild active={active} {...common}><Link href={href}>{content}</Link></NavigationItem>;
  return collapsed
    ? <Tooltip><TooltipTrigger asChild>{item}</TooltipTrigger><TooltipContent side="right">{label}</TooltipContent></Tooltip>
    : item;
}
