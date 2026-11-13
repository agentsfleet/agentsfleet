"use client";

import Link from "next/link";
import { SidebarSimpleIcon } from "@phosphor-icons/react/ssr";
import { BrandMark, Button, TooltipButton, cn } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { shellSidebarState } from "./shell-sidebar-state";

export function ShellControls({workspaceId, collapsed}: {workspaceId: string | null; collapsed: boolean}) {
  return <div data-testid="shell-leading-cluster" className="flex h-12 shrink-0 items-center justify-center px-sm md:px-lg">
    <Button asChild variant="ghost" className={cn("h-11 w-11 min-w-0 justify-center px-0 hover:bg-muted hover:text-foreground [&_svg]:size-6 md:h-12", !collapsed && "md:w-full md:justify-start md:gap-lg md:px-lg")} aria-label="agentsfleet home">
      <Link href={workspaceId ? workspacePath(workspaceId, "fleets/new") : "/"} prefetch={false}>
        <BrandMark className="size-6 shrink-0 rounded-brand bg-brand-tile p-xs text-brand-ray" />
        {!collapsed ? <span className="hidden text-label font-control md:inline">agentsfleet</span> : null}
      </Link>
    </Button>
  </div>;
}

export function SidebarExpandControl({collapsed}: {collapsed: boolean}) {
  const label = collapsed ? "Expand navigation" : "Collapse navigation";
  return <TooltipButton type="button" variant="ghost" tooltip={collapsed ? label : null} aria-label={label}
    aria-expanded={!collapsed} aria-controls="app-sidebar-nav" onClick={shellSidebarState.toggle}
    className={cn("hidden h-12 min-h-12 w-full items-center gap-lg md:inline-flex hover:bg-muted hover:text-foreground [&_svg]:size-5", collapsed ? "justify-center px-0" : "justify-start px-lg")}>
    <SidebarSimpleIcon size={20} className="size-5" aria-hidden="true" />{collapsed ? null : label}
  </TooltipButton>;
}
