"use client";

import { Suspense, useState } from "react";
import { SidebarSimpleIcon } from "@phosphor-icons/react/ssr";
import { Button, Dialog, DialogContent, DialogTitle, DialogTrigger, Nav } from "@agentsfleet/design-system";
import { NavSection } from "./SidebarNavigationSections";
import { PRIMARY_NAV, resolveActiveHref, resolveHref } from "./sidebar-navigation-data";
import { DraftSidebarHistory } from "./DraftSidebarHistory";

export function MobileNavigation({pathname, workspaceId, account}: {pathname: string; workspaceId: string | null; account: string | null}) {
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);
  const activeHref = resolveActiveHref(PRIMARY_NAV.map(entry => resolveHref(entry, workspaceId)), pathname);
  return <Dialog open={open} onOpenChange={setOpen}>
    <DialogTrigger asChild><Button variant="ghost" className="min-h-11 w-full px-0 hover:bg-muted hover:text-foreground md:hidden [&_svg]:size-5" aria-label="Open navigation"><SidebarSimpleIcon size={20} aria-hidden="true" /></Button></DialogTrigger>
    <DialogContent aria-describedby={undefined} className="max-w-sm p-xl">
      <DialogTitle className="mb-xl">Navigation</DialogTitle>
      <Nav aria-label="Expanded navigation" className="space-y-xl">
        <NavSection items={PRIMARY_NAV} activeHref={activeHref} workspaceId={workspaceId} collapsed={false} overlay onNavigate={close} />
        <Suspense><DraftSidebarHistory account={account} workspace={workspaceId} onNavigate={close} /></Suspense>
      </Nav>
    </DialogContent>
  </Dialog>;
}
