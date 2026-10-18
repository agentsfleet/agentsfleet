"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef } from "react";
import { ArrowLeftIcon, ChevronRightIcon, ShieldCheckIcon, LogOutIcon, SettingsIcon } from "lucide-react";
import { cn, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator } from "@agentsfleet/design-system";
import { AuthSignOutButton } from "@/lib/auth/client";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { ACCOUNT_NAV, BOTTOM_NAV, PLATFORM_NAV, WORKSPACE_NAV, NAV_SURFACE, navSource, resolveActiveHref, resolveHref, type NavEntry } from "./sidebar-navigation-data";

export const MENU_VIEW = { home: "home", settings: "settings", platform: "platform" } as const;
export type MenuView = typeof MENU_VIEW[keyof typeof MENU_VIEW];
type Props = { view: MenuView; onView: (view: MenuView) => void; workspaceId: string | null; operatorScopes: string[] };

export function AccountMenuNavigation({ view, onView, workspaceId, operatorScopes }: Props) {
  const backRef = useRef<HTMLDivElement>(null);
  useEffect(() => { backRef.current?.focus(); }, [view]);
  const platform = PLATFORM_NAV.filter((entry) => !entry.scope || operatorScopes.includes(entry.scope));
  if (view === MENU_VIEW.home) return <>
    <DropdownMenuSeparator />
    <MenuDestination label="Settings" Icon={SettingsIcon} onSelect={() => onView(MENU_VIEW.settings)} />
    {platform.length > 0 ? <MenuDestination label="Platform" Icon={ShieldCheckIcon} onSelect={() => onView(MENU_VIEW.platform)} /> : null}
    <MenuLinks items={BOTTOM_NAV} workspaceId={workspaceId} />
    <DropdownMenuSeparator />
    <AuthSignOutButton redirectUrl="/sign-in"><DropdownMenuItem><LogOutIcon size={18} strokeWidth={1.75} className="shrink-0" aria-hidden="true" />Sign out</DropdownMenuItem></AuthSignOutButton>
  </>;
  return <>
    <DropdownMenuItem ref={backRef} onSelect={(event) => { event.preventDefault(); onView(MENU_VIEW.home); }}>
      <ArrowLeftIcon size={18} strokeWidth={1.75} className="shrink-0" aria-hidden="true" />Back
    </DropdownMenuItem>
    <DropdownMenuSeparator />
    {view === MENU_VIEW.settings ? <>
      <DropdownMenuLabel>Workspace</DropdownMenuLabel>
      <MenuLinks items={WORKSPACE_NAV} workspaceId={workspaceId} />
      <DropdownMenuSeparator />
      <DropdownMenuLabel>Account</DropdownMenuLabel>
      <MenuLinks items={ACCOUNT_NAV} workspaceId={workspaceId} />
    </> : <><DropdownMenuLabel>Platform</DropdownMenuLabel><MenuLinks items={platform} workspaceId={workspaceId} /></>}
  </>;
}

function MenuDestination({ label, Icon, onSelect }: { label: string; Icon: typeof SettingsIcon; onSelect: () => void }) {
  return <DropdownMenuItem onSelect={(event) => { event.preventDefault(); onSelect(); }}>
    <Icon size={18} strokeWidth={1.75} className="shrink-0" aria-hidden="true" /><span className="flex-1">{label}</span><ChevronRightIcon size={14} />
  </DropdownMenuItem>;
}

function MenuLinks({ items, workspaceId }: { items: NavEntry[]; workspaceId: string | null }) {
  const pathname = usePathname();
  const activeHref = resolveActiveHref(items.map((entry) => resolveHref(entry, workspaceId)), pathname);
  return items.map((entry) => {
    const href = resolveHref(entry, workspaceId);
    return <DropdownMenuItem key={entry.path} asChild onSelect={() => trackNavigationClicked({ source: navSource(entry), surface: NAV_SURFACE, target: href })}>
      <Link href={href} aria-current={href === activeHref ? "page" : undefined}
        className={cn(href === activeHref && "bg-accent text-primary")}
        {...(entry.external ? { target: "_blank", rel: "noopener noreferrer" } : {})}>
        <entry.icon size={18} strokeWidth={1.75} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="flex-1">{entry.label}</span>
      </Link>
    </DropdownMenuItem>;
  });
}
