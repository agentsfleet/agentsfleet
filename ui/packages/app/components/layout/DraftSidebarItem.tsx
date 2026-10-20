"use client";

import type { SidebarDraft } from "@/lib/api/draft-summary";
import Link from "next/link";
import { IconAction, NavItem } from "@agentsfleet/design-system";
import { MessageSquareIcon, Trash2Icon } from "lucide-react";
import { workspacePath } from "@/lib/workspace-routes";
import { NAV_ICON_SIZE, NAV_ICON_STROKE_WIDTH } from "./sidebar-navigation-data";

type Props = {card: SidebarDraft; workspace: string; active: boolean; disabled: boolean; onNavigate: () => void;
  onDelete: () => void};

export function DraftSidebarItem({card, workspace, active, disabled, onNavigate, onDelete}: Props) {
  return <div className="group relative">
    <NavItem asChild active={active} onClick={onNavigate} className="pr-10">
      <Link href={`${workspacePath(workspace, "fleets/new")}?draft=${encodeURIComponent(card.id)}`} prefetch={false} aria-label={card.title} aria-describedby={`${card.id}-state`} title={card.title}>
        <MessageSquareIcon size={NAV_ICON_SIZE} strokeWidth={NAV_ICON_STROKE_WIDTH} className="shrink-0 text-muted-foreground" aria-hidden="true" />
        <span className="min-w-0"><span className="block truncate">{card.title}</span><span id={`${card.id}-state`} className="block truncate text-label text-muted-foreground">{card.detail}</span></span>
      </Link>
    </NavItem>
    <IconAction variant="ghost" label={`Delete chat: ${card.title}`} disabled={disabled} className="absolute right-0 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
      onClick={onDelete}><Trash2Icon size={14} aria-hidden="true" /></IconAction>
  </div>;
}
