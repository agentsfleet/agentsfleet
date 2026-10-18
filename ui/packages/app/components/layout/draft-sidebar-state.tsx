"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type Dispatch, type SetStateAction, type ReactNode } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import type { DraftClient } from "@/lib/api/drafts";
import type { DeleteDraft } from "@/lib/api/draft-request";
import type { DraftView } from "@/lib/api/draft-state";
import { firstChatRequest, sidebarDrafts, type SidebarDraft } from "@/lib/api/draft-summary";
import { DRAFT_LIST, DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";

export type DraftSidebarState = { workspace: string; subject: string; cards: readonly SidebarDraft[]; fresh: (() => void) | null; remove: DeleteDraft | null; owner: DraftClient | null; locked: boolean; current: string | null;
  more: (() => Promise<void>) | null; status: DraftHistoryStatus };
type DraftHistoryStatus = typeof HISTORY_STATUS[keyof typeof HISTORY_STATUS];
export const HistoryContext = createContext<DraftSidebarState | null>(null);
export const UpdateContext = createContext<Dispatch<SetStateAction<DraftSidebarState | null>>>(() => {});

export function DraftSidebarProvider({ children }: { children: ReactNode }) {
  const [history, setHistory] = useState<DraftSidebarState | null>(null);
  return <UpdateContext value={setHistory}><HistoryContext value={history}>{children}</HistoryContext></UpdateContext>;
}

export function useDraftSidebarBridge(client: DraftClient, view: DraftView, workspace: string, subject: string, running: boolean) {
  const update = useContext(UpdateContext);
  const fresh = useCallback(() => client.fresh(), [client]);
  const remove = useCallback((id: string, signal: AbortSignal) => client.remove(id, signal), [client]);
  const loadChats = useCallback(() => client.loadMore(DRAFT_LIST.chats), [client]);
  const current = view.draft?.id ?? null;
  const request = firstChatRequest(view.messages);
  const source = view.cards;
  const inaccessible = view.inaccessible;
  const locked = view.busy || view.pending || inaccessible || running || view.listStatus !== HISTORY_STATUS.ready;
  const more = view.moreChats && !inaccessible ? loadChats : null;
  const cards = useMemo(() => inaccessible ? [] : sidebarDrafts(source, view.draft, request),
    [source, view.draft, request, inaccessible]);
  const status = view.listStatus;
  useEffect(() => {
    update((previous) => {
      if (previous?.workspace === workspace && previous.subject === subject && previous.fresh === fresh && previous.remove === remove && previous.locked === locked && previous.current === current && previous.more === more && previous.status === status
        && previous.cards.length === cards.length && previous.cards.every((card, index) => card.id === cards[index]?.id && card.title === cards[index].title && card.detail === cards[index].detail)) return previous;
      return { workspace, subject, cards: status === HISTORY_STATUS.loading && previous?.workspace === workspace && previous.subject === subject ? previous.cards : cards, fresh, remove, locked, current, more, status, owner: client };
    });
  }, [update, workspace, subject, cards, fresh, remove, locked, current, more, status, client]);
  useEffect(() => () => update((current) => current?.owner !== client ? current : current.status !== HISTORY_STATUS.loading
    ? {...current, fresh: null, remove: null, owner: null, current: null, more: null, locked: current.status !== HISTORY_STATUS.ready} : null), [update, client]);
}

export function useFreshFleetChat(workspace: string | null) {
  const history = useContext(HistoryContext);
  const { userId } = useCurrentUser();
  return () => { if (history?.workspace === workspace && history.subject === userId && !history.locked) history.fresh?.(); };
}

// A late delete completion may only update the private history that owned it.
export function removeSidebarDraft(value: DraftSidebarState | null, scope: Pick<DraftSidebarState, "workspace" | "subject">, id: string): DraftSidebarState | null {
  if (!value || value.workspace !== scope.workspace || value.subject !== scope.subject) return value;
  return {...value, cards: value.cards.filter(card => card.id !== id), current: value.current === id ? null : value.current};
}
