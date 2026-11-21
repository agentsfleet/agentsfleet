"use client";

import { useContext } from "react";
import { HistoryContext } from "./draft-sidebar-state";
import dynamic from "next/dynamic";
import { SectionLabel } from "@agentsfleet/design-system";
import { CaretDownIcon } from "@phosphor-icons/react/ssr";
import { useCurrentUser } from "@/lib/auth/client";

const RecentFleetList = dynamic(() => import("./RecentChats").then(module => module.RecentFleetList));
export function DraftSidebarHistory({account = null, workspace, onNavigate}: {account?: string | null; workspace: string | null; onNavigate: () => void}) {
  const {userId} = useCurrentUser();
  const history = useContext(HistoryContext);
  if (!workspace || !userId || !account) return null;
  return <div key={`${account}:${workspace}:${userId}`} className="ph-no-capture ph-no-record px-sm pt-xl">
    <details open className="group/fleets">
      <summary className="mb-sm flex min-h-11 cursor-pointer list-none items-center justify-between rounded-control px-md focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">
        <SectionLabel variant="navigation">Fleets</SectionLabel>
        <CaretDownIcon size={14} className="-rotate-90 group-open/fleets:rotate-0" aria-hidden="true" />
      </summary>
      <RecentFleetList history={history?.owner && history.workspace === workspace && history.subject === userId ? history : null} workspace={workspace} onNavigate={onNavigate} />
    </details>
  </div>;
}
