"use client";

import { useContext, useEffect, useMemo } from "react";
import { Alert, Button } from "@agentsfleet/design-system";
import { DRAFT_SEARCH_STATUS as STATUS } from "@/lib/api/draft-vocabulary";
import { sidebarDrafts } from "@/lib/api/draft-summary";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { DraftChatList } from "./DraftChatList";
import { UpdateContext, type DraftSidebarState } from "./draft-sidebar-state";
import { useDraftSearch } from "./use-draft-search";
export type DraftSearchResultsProps = {history: DraftSidebarState; query: string; onNavigate: () => void};

export function DraftSearchResults({history, query, onNavigate}: DraftSearchResultsProps) {
  const search = useDraftSearch(history.workspace, query, history.cards, history.searchVersion);
  const update = useContext(UpdateContext);
  const cards = useMemo(() => sidebarDrafts(search.view.items, null), [search.view.items]);
  const inaccessible = search.view.status === STATUS.inaccessible;
  useEffect(() => {
    if (!inaccessible) return;
    history.owner?.invalidateAccess();
    update(value => value?.workspace === history.workspace && value.subject === history.subject
      ? {...value, cards: [], more: null, fresh: null, remove: null, current: null, locked: true, status: "unavailable"} : value);
  }, [inaccessible, update, history.workspace, history.subject, history.owner]);
  if (search.view.status === STATUS.loading) return <output className="block px-md py-sm text-label text-muted-foreground">Searching chats…</output>;
  if (search.view.status === STATUS.failed || search.view.status === STATUS.inaccessible) return <Alert variant="destructive">
    {search.view.message}{search.view.status === STATUS.failed ? <Button variant="ghost" size="sm" onClick={search.retry}>Retry search</Button> : null}
  </Alert>;
  return <>
    <DraftChatList history={history} cards={cards} onNavigate={onNavigate} onRemoved={search.remove} />
    {cards.length === 0 ? <p className="px-md py-sm text-label text-muted-foreground">No matching chats</p> : null}
    {search.view.next_cursor !== null ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={search.more} /> : null}
  </>;
}
