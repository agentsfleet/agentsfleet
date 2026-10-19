"use client";

import { createContext, useContext, useState, type Dispatch, type SetStateAction, type ReactNode } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import type { DraftClient } from "@/lib/api/drafts";
import type { DeleteDraft } from "@/lib/api/draft-request";
import type { SidebarDraft } from "@/lib/api/draft-summary";
import type { DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";

export type DraftSidebarState = { workspace: string; subject: string; cards: readonly SidebarDraft[]; fresh: (() => void) | null; remove: DeleteDraft | null; owner: DraftClient | null; locked: boolean; current: string | null; searchVersion: string | null;
  more: (() => Promise<void>) | null; status: DraftHistoryStatus };
type DraftHistoryStatus = typeof HISTORY_STATUS[keyof typeof HISTORY_STATUS];
export const HistoryContext = createContext<DraftSidebarState | null>(null);
export const UpdateContext = createContext<Dispatch<SetStateAction<DraftSidebarState | null>>>(() => {});

export function DraftSidebarProvider({ children }: { children: ReactNode }) {
  const [history, setHistory] = useState<DraftSidebarState | null>(null);
  return <UpdateContext value={setHistory}><HistoryContext value={history}>{children}</HistoryContext></UpdateContext>;
}

export function useFreshFleetChat(workspace: string | null) {
  const history = useContext(HistoryContext);
  const { userId } = useCurrentUser();
  return () => { if (history?.workspace === workspace && history.subject === userId && !history.locked) history.fresh?.(); };
}

// A late delete completion may only update the private history that owned it.
export function removeSidebarDraft(value: DraftSidebarState | null, scope: Pick<DraftSidebarState, "workspace" | "subject">, id: string): DraftSidebarState | null {
  if (!value || value.workspace !== scope.workspace || value.subject !== scope.subject) return value;
  if (value.current !== id && !value.cards.some(card => card.id === id)) return value;
  return {...value, cards: value.cards.filter(card => card.id !== id), current: value.current === id ? null : value.current};
}
