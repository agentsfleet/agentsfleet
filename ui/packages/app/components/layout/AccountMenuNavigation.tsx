"use client";

import { AppearanceMenuItem } from "./AppearanceMenuItem";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { SignOutIcon } from "@phosphor-icons/react/ssr";
import { cn, DropdownMenuItem, DropdownMenuSeparator } from "@agentsfleet/design-system";
import { AuthSignOutButton } from "@/lib/auth/client";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { NAV_SURFACE, NAV_ICON_SIZE, navSource, resolveActiveHref, resolveHref, type NavEntry } from "./sidebar-navigation-data";
import { ACCOUNT_NAV, ACCOUNT_SHORTCUTS, SERVICE_SHORTCUTS, BOTTOM_NAV, WORKSPACE_NAV } from "./account-menu-data";
import type { TenantBilling } from "@/lib/types";
import { formatHeaderBalance } from "./BalanceLink";

type Props = {workspaceId: string | null; billing?: TenantBilling | null};

export function AccountMenuNavigation({workspaceId, billing}: Props) {
  return <>
    <DropdownMenuSeparator />
    <MenuLinks items={ACCOUNT_NAV} workspaceId={workspaceId} />
    <MenuLinks items={ACCOUNT_SHORTCUTS} workspaceId={workspaceId} billing={billing} />
    <MenuLinks items={WORKSPACE_NAV} workspaceId={workspaceId} />
    <AppearanceMenuItem />
    <DropdownMenuSeparator />
    <MenuLinks items={SERVICE_SHORTCUTS} workspaceId={workspaceId} />
    <MenuLinks items={BOTTOM_NAV} workspaceId={workspaceId} />
    <DropdownMenuSeparator />
    <AuthSignOutButton redirectUrl="/sign-in"><DropdownMenuItem><SignOutIcon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" />Sign out</DropdownMenuItem></AuthSignOutButton>
  </>;
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
