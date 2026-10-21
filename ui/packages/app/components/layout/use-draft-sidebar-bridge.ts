"use client";

import { useCallback, useContext, useEffect, useMemo } from "react";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { DraftSidebarState } from "./draft-sidebar-state";
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
    const next = {workspace, subject, cards, fresh, remove, locked, current, more, status, owner: client};
    update((previous) => reconcileSidebar(previous, next, inaccessible));
  }, [update, workspace, subject, cards, fresh, remove, locked, current, more, status, client, inaccessible]);
  useEffect(() => () => update((current) => releaseSidebar(current, client)), [update, client]);
}

function reconcileSidebar(previous: DraftSidebarState | null, next: DraftSidebarState, inaccessible: boolean): DraftSidebarState {
  if (!previous || previous.workspace !== next.workspace || previous.subject !== next.subject) return next;
  const keepCards = !inaccessible && (next.status === HISTORY_STATUS.loading || sameCards(previous.cards, next.cards));
  const cards = keepCards ? previous.cards : next.cards;
  if (previous.fresh === next.fresh && previous.remove === next.remove && previous.locked === next.locked
    && previous.current === next.current && previous.more === next.more && previous.status === next.status
    && previous.cards === cards && previous.owner === next.owner) return previous;
  return {...next, cards};
}

function releaseSidebar(current: DraftSidebarState | null, owner: DraftClient): DraftSidebarState | null {
  if (current?.owner !== owner) return current;
  if (current.status === HISTORY_STATUS.loading) return null;
  return {...current, fresh: null, remove: null, owner: null, current: null, more: null, locked: current.status !== HISTORY_STATUS.ready};
}

function sameCards(left: readonly SidebarDraft[], right: readonly SidebarDraft[]): boolean {
  return left.length === right.length && left.every((card, index) => card.id === right[index]?.id && card.title === right[index].title && card.detail === right[index].detail);
}
