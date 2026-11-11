"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef } from "react";
import { ArrowLeftIcon, CaretRightIcon, SignOutIcon, GearIcon } from "@phosphor-icons/react/ssr";
import { cn, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator } from "@agentsfleet/design-system";
import { AuthSignOutButton } from "@/lib/auth/client";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { NAV_SURFACE, NAV_ICON_SIZE, navSource, resolveActiveHref, resolveHref, type NavEntry } from "./sidebar-navigation-data";
import { ACCOUNT_NAV, ACCOUNT_SHORTCUTS, SERVICE_SHORTCUTS, BOTTOM_NAV, WORKSPACE_NAV } from "./account-menu-data";
import type { TenantBilling } from "@/lib/types";
import { formatHeaderBalance } from "./BalanceLink";

export const MENU_VIEW = { home: "home", settings: "settings" } as const;
export type MenuView = typeof MENU_VIEW[keyof typeof MENU_VIEW];
type Props = { view: MenuView; onView: (view: MenuView) => void; workspaceId: string | null; billing?: TenantBilling | null };

export function AccountMenuNavigation({ view, onView, workspaceId, billing }: Props) {
  const backRef = useRef<HTMLDivElement>(null);
  useEffect(() => { backRef.current?.focus(); }, [view]);
  if (view === MENU_VIEW.home) return <>
    <DropdownMenuSeparator />
    <MenuDestination label="Settings" Icon={GearIcon} onSelect={() => onView(MENU_VIEW.settings)} />
    <MenuLinks items={ACCOUNT_SHORTCUTS} workspaceId={workspaceId} billing={billing} />
    <DropdownMenuSeparator />
    <MenuLinks items={SERVICE_SHORTCUTS} workspaceId={workspaceId} />
    <MenuLinks items={BOTTOM_NAV} workspaceId={workspaceId} />
    <DropdownMenuSeparator />
    <AuthSignOutButton redirectUrl="/sign-in"><DropdownMenuItem><SignOutIcon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" />Sign out</DropdownMenuItem></AuthSignOutButton>
  </>;
  return <>
    <DropdownMenuItem ref={backRef} onSelect={(event) => { event.preventDefault(); onView(MENU_VIEW.home); }}>
      <ArrowLeftIcon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" />Back
    </DropdownMenuItem>
    <DropdownMenuSeparator />
    <>
      <DropdownMenuLabel>Workspace</DropdownMenuLabel>
      <MenuLinks items={WORKSPACE_NAV} workspaceId={workspaceId} />
      <DropdownMenuSeparator />
      <DropdownMenuLabel>Account</DropdownMenuLabel>
      <MenuLinks items={ACCOUNT_NAV} workspaceId={workspaceId} />
    </>
  </>;
}

function MenuDestination({ label, Icon, onSelect }: { label: string; Icon: typeof GearIcon; onSelect: () => void }) {
  return <DropdownMenuItem onSelect={(event) => { event.preventDefault(); onSelect(); }}>
    <Icon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="flex-1">{label}</span><CaretRightIcon size={14} className="text-muted-foreground" aria-hidden="true" />
  </DropdownMenuItem>;
}

function MenuLinks({ items, workspaceId, billing }: { items: NavEntry[]; workspaceId: string | null; billing?: TenantBilling | null }) {
  const pathname = usePathname();
  const activeHref = resolveActiveHref(items.map((entry) => resolveHref(entry, workspaceId)), pathname);
  return items.map((entry) => {
    const href = resolveHref(entry, workspaceId);
    return <DropdownMenuItem key={entry.path} asChild onSelect={() => trackNavigationClicked({ source: navSource(entry), surface: NAV_SURFACE, target: href })}>
      <Link href={href} aria-current={href === activeHref ? "page" : undefined}
        className={cn(href === activeHref && "bg-accent text-primary")}
        {...(entry.external ? { target: "_blank", rel: "noopener noreferrer" } : {})}>
        <entry.icon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="flex-1">{entry.label}</span>
        {entry.path === "/settings/billing" && billing ? <span aria-label="Credit balance" className="text-eyebrow tabular-nums text-muted-foreground">{formatHeaderBalance(billing.balance_nanos)}</span> : null}
      </Link>
    </DropdownMenuItem>;
  });
}
