"use client";

import dynamic from "next/dynamic";
import { SectionLabel } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";

const RecentFleetList = dynamic(() => import("./RecentChats").then(module => module.RecentFleetList));
export function DraftSidebarHistory({account = null, workspace, onNavigate}: {account?: string | null; workspace: string | null; onNavigate: () => void}) {
  const {userId} = useCurrentUser();
  if (!workspace || !userId || !account) return null;
  return <div key={`${account}:${workspace}:${userId}`} className="ph-no-capture ph-no-record px-sm pt-xl">
    <SectionLabel variant="navigation" className="mb-sm px-md">Fleets</SectionLabel>
    <RecentFleetList workspace={workspace} onNavigate={onNavigate} />
  </div>;
}
