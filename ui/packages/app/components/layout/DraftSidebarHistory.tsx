"use client";

import { useContext, useState } from "react";
import { Input, SectionLabel } from "@agentsfleet/design-system";
import { SearchIcon } from "lucide-react";
import { useCurrentUser } from "@/lib/auth/client";
import { DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import { DRAFT_SEARCH_QUERY_MAX } from "@/lib/api/draft-vocabulary";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { DraftChatList } from "./DraftChatList";
import { HistoryContext, type DraftSidebarState } from "./draft-sidebar-state";
import { DraftSearchResultsDynamic } from "./DraftSearchResultsDynamic";

const HISTORY_COPY = {
  heading: "Fleet setup chats", search: "Search all chats", empty: "Your fleet setup conversations will appear here.",
  loading: "Loading setup chats…", unavailable: "Chat history is unavailable.",
} as const;

export function DraftSidebarHistory({workspace, onNavigate}: {workspace: string | null; onNavigate: () => void}) {
  const history = useContext(HistoryContext);
  const {userId} = useCurrentUser();
  if (!history || history.subject !== userId || history.workspace !== workspace || !workspace) return null;
  return <SidebarChats key={`${workspace}:${userId}`} history={history} onNavigate={onNavigate} />;
}

function SidebarChats({history, onNavigate}: {history: DraftSidebarState; onNavigate: () => void}) {
  const [query, setQuery] = useState("");
  const searching = history.status === HISTORY_STATUS.ready && query.trim().length > 0;
  return <div className="ph-no-capture ph-no-record px-sm pt-xl">
    <SectionLabel variant="navigation" className="mb-sm px-md">{HISTORY_COPY.heading}</SectionLabel>
    {history.cards.length > 0 || searching ? <div className="relative mb-sm">
      <SearchIcon size={14} className="pointer-events-none absolute left-3 top-3 text-muted-foreground" aria-hidden="true" />
      <Input aria-label={HISTORY_COPY.search} placeholder={HISTORY_COPY.search} value={query} maxLength={DRAFT_SEARCH_QUERY_MAX}
        onChange={event => setQuery(event.target.value)} className="pl-9" />
    </div> : null}
    {searching ? <DraftSearchResultsDynamic history={history} query={query} onNavigate={onNavigate} /> : <>
      <DraftChatList history={history} cards={history.cards} onNavigate={onNavigate} />
      {!history.cards.length ? <p className="px-md py-sm text-label text-muted-foreground">{history.status === HISTORY_STATUS.unavailable ? HISTORY_COPY.unavailable
        : history.status === HISTORY_STATUS.loading ? HISTORY_COPY.loading : HISTORY_COPY.empty}</p> : null}
      {history.more ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
    </>}
  </div>;
}
