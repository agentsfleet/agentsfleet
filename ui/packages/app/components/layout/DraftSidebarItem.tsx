"use client";

import type { SidebarDraft } from "@/lib/api/draft-summary";
import Link from "next/link";
import { IconAction, NavItem } from "@agentsfleet/design-system";
import { MessageSquareIcon, Trash2Icon } from "lucide-react";
import { workspacePath } from "@/lib/workspace-routes";

type Props = {card: SidebarDraft; workspace: string; active: boolean; disabled: boolean; onNavigate: () => void;
  onDelete: () => void};

export function DraftSidebarItem({card, workspace, active, disabled, onNavigate, onDelete}: Props) {
  return <div className="group relative">
    <NavItem asChild active={active} onClick={onNavigate} className="pr-10">
      <Link href={`${workspacePath(workspace, "fleets/new")}?draft=${encodeURIComponent(card.id)}`} prefetch={false} aria-label={card.title} aria-describedby={`${card.id}-state`} title={card.title}>
        <MessageSquareIcon size={14} className="shrink-0" aria-hidden="true" />
        <span className="min-w-0"><span className="block truncate">{card.title}</span><span id={`${card.id}-state`} className="block truncate text-eyebrow text-muted-foreground">{card.detail}</span></span>
      </Link>
    </NavItem>
    <IconAction variant="ghost" label={`Delete chat: ${card.title}`} disabled={disabled} className="absolute right-0 top-sm"
      onClick={onDelete}><Trash2Icon size={14} aria-hidden="true" /></IconAction>
  </div>;
}
