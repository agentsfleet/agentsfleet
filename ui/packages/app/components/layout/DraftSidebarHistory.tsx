"use client";

import { useContext } from "react";
import { HistoryContext } from "./draft-sidebar-state";
import dynamic from "next/dynamic";
import { NavItem, TooltipButton } from "@agentsfleet/design-system";
import { CaretDownIcon, StackIcon } from "@phosphor-icons/react/ssr";
import { useCurrentUser } from "@/lib/auth/client";
import { shellSidebarState } from "./shell-sidebar-state";

const RecentFleetList = dynamic(() => import("./RecentChats").then(module => module.RecentFleetList));
export function DraftSidebarHistory({account = null, workspace, onNavigate, overlay = false}: {account?: string | null; workspace: string | null; onNavigate: () => void; overlay?: boolean}) {
  const {userId} = useCurrentUser();
  const history = useContext(HistoryContext);
  if (!workspace || !userId || !account) return null;
  return <div key={`${account}:${workspace}:${userId}`} className={overlay ? "ph-no-capture ph-no-record px-sm" : "ph-no-capture ph-no-record px-sm md:px-lg"}>
    <details open className="group/fleets">
      <NavItem asChild appearance="list" className="cursor-pointer list-none text-eyebrow font-medium leading-eyebrow text-text-dim [&::-webkit-details-marker]:hidden"><summary>
        <span className="flex-1">Your Fleets</span>
        <CaretDownIcon size={14} className="shrink-0 -rotate-90 transition-transform group-open/fleets:rotate-0 motion-reduce:transition-none" aria-hidden="true" />
      </summary></NavItem>
      <RecentFleetList history={history?.owner && history.workspace === workspace && history.subject === userId ? history : null} workspace={workspace} onNavigate={onNavigate} />
    </details>
  </div>;
}

export function CollapsedFleetListTrigger() {
  return <TooltipButton variant="ghost" tooltip="Your Fleets" aria-label="Your Fleets" onClick={shellSidebarState.toggle}
    className="mx-auto hidden size-12 px-0 hover:bg-muted hover:text-foreground md:inline-flex [&_svg]:size-5"><StackIcon size={20} aria-hidden="true" /></TooltipButton>;
}
