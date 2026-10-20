"use client";

import { useCallback, useContext, useEffect, useMemo } from "react";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { firstChatRequest, sidebarDrafts, type SidebarDraft } from "@/lib/api/draft-summary";
import { DRAFT_LIST, DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import { UpdateContext } from "./draft-sidebar-state";

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
  const setupRevision = view.fleet?.id === view.draft?.linked_fleet_id ? view.fleet?.setup_revision ?? null : null;
  const cards = useMemo(() => inaccessible ? [] : sidebarDrafts(source, view.draft, request, setupRevision),
    [source, view.draft, request, inaccessible, setupRevision]);
  const status = view.listStatus;
  useEffect(() => {
    update((previous) => {
      const sameScope = previous?.workspace === workspace && previous.subject === subject;
      const nextCards = sameScope && !inaccessible && (status === HISTORY_STATUS.loading || sameCards(previous.cards, cards)) ? previous.cards : cards;
      if (sameScope && previous.fresh === fresh && previous.remove === remove && previous.locked === locked && previous.current === current && previous.more === more && previous.status === status
        && previous.cards === nextCards && previous.owner === client) return previous;
      return { workspace, subject, cards: nextCards, fresh, remove, locked, current, more, status, owner: client };
    });
  }, [update, workspace, subject, cards, fresh, remove, locked, current, more, status, client, inaccessible]);
  useEffect(() => () => update((current) => current?.owner !== client ? current : current.status !== HISTORY_STATUS.loading
    ? {...current, fresh: null, remove: null, owner: null, current: null, more: null, locked: current.status !== HISTORY_STATUS.ready} : null), [update, client]);
}

function sameCards(left: readonly SidebarDraft[], right: readonly SidebarDraft[]): boolean {
  return left.length === right.length && left.every((card, index) => card.id === right[index]?.id && card.title === right[index].title && card.detail === right[index].detail);
}
