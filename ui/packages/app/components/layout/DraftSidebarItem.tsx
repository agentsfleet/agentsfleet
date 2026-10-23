"use client";

import type { SidebarDraft } from "@/lib/api/draft-summary";
import Link from "next/link";
import { IconAction, NavItem, Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@agentsfleet/design-system";
import { ChevronRightIcon, Trash2Icon } from "lucide-react";
import { CHAT_ACTIVITY } from "@/lib/api/draft-summary";
import { workspacePath } from "@/lib/workspace-routes";
import { NAV_ICON_STROKE_WIDTH } from "./sidebar-navigation-data";

type Props = {card: SidebarDraft; workspace: string; active: boolean; disabled: boolean; onNavigate: () => void;
  onDelete: () => void};
const CHAT_ICON_SIZE = 14;

export function DraftSidebarItem({card, workspace, active, disabled, onNavigate, onDelete}: Props) {
  const status = card.activity === CHAT_ACTIVITY.pending ? "Waiting for a result" : card.activity === CHAT_ACTIVITY.failed ? "Setup message failed" : card.detail;
  return <div className="group relative">
    <TooltipProvider><Tooltip><TooltipTrigger asChild><NavItem asChild active={active} onClick={onNavigate} className="group/chat min-h-12 items-center gap-md rounded-lg border border-border px-md py-md pr-12 text-foreground">
      <Link href={`${workspacePath(workspace, "fleets/new")}?draft=${encodeURIComponent(card.id)}`} prefetch={false} aria-label={card.title} aria-describedby={`${card.id}-state`} title={card.title}>
        <span className="min-w-0 flex-1 truncate leading-body-sm text-foreground">{card.title}</span>
        <span id={`${card.id}-state`} className="sr-only">{status}</span>
        <span className="inline-flex shrink-0 items-center gap-sm text-eyebrow text-pulse">Resume<ChevronRightIcon className="size-3.5" aria-hidden="true" /></span>
      </Link>
    </NavItem></TooltipTrigger><TooltipContent side="right" className="ph-no-capture ph-no-record max-w-xs break-words"><p>{card.title}</p><p className="text-muted-foreground">{status}</p></TooltipContent></Tooltip></TooltipProvider>
    <IconAction variant="ghost" label={`Delete chat: ${card.title}`} disabled={disabled} className="absolute right-xs top-1/2 -translate-y-1/2 text-muted-foreground opacity-0 transition-opacity duration-snap group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
      onClick={onDelete}><Trash2Icon size={CHAT_ICON_SIZE} strokeWidth={NAV_ICON_STROKE_WIDTH} aria-hidden="true" /></IconAction>
  </div>;
}
