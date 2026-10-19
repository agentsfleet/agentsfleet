"use client";

import { useContext, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import type { SidebarDraft } from "@/lib/api/draft-summary";
import { workspacePath } from "@/lib/workspace-routes";
import { DraftSidebarItem } from "./DraftSidebarItem";
import { DraftDeleteDialogDynamic } from "./DraftDeleteDialogDynamic";
import { UpdateContext, removeSidebarDraft, type DraftSidebarState } from "./draft-sidebar-state";

type Props = {history: DraftSidebarState; cards: readonly SidebarDraft[]; onNavigate: () => void; onRemoved?: (id: string) => void};

export function DraftChatList({history, cards, onNavigate, onRemoved}: Props) {
  const update = useContext(UpdateContext);
  const router = useRouter();
  const search = useSearchParams();
  const [selected, setSelected] = useState<SidebarDraft | null>(null);
  const active = search.get("draft") ?? history.current;
  const removed = (id: string): void => {
    update(value => removeSidebarDraft(value, history, id));
    if (id === active) router.replace(workspacePath(history.workspace, "fleets/new"));
    onRemoved?.(id); setSelected(null);
  };
  return <>
    {cards.map(card => <DraftSidebarItem key={card.id} card={card} workspace={history.workspace} active={card.id === active}
      disabled={history.locked || selected !== null} onNavigate={onNavigate} onDelete={() => setSelected(card)} />)}
    {selected ? <DraftDeleteDialogDynamic card={selected} workspace={history.workspace} remove={history.remove}
      onRemoved={removed} onClose={() => setSelected(null)} /> : null}
  </>;
}
