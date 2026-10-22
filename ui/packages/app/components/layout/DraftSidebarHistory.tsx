"use client";

import { useContext } from "react";
import { SectionLabel } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { DraftChatList } from "./DraftChatList";
import { HistoryContext, type DraftSidebarState } from "./draft-sidebar-state";
import { RecentFleetList, StandaloneDraftHistory } from "./RecentChats";

const HISTORY_COPY = {
  heading: "Recent", empty: "Your setup chats will appear here.",
  loading: "Loading setup chats…", unavailable: "Chat history is unavailable.",
} as const;

export function DraftSidebarHistory({account = null, workspace, onNavigate}: {account?: string | null; workspace: string | null; onNavigate: () => void}) {
  const history = useContext(HistoryContext);
  const {userId} = useCurrentUser();
  if (!workspace || !userId) return null;
  const current = history?.subject === userId && history.workspace === workspace ? history : null;
  if (!current && !account) return null;
  return <div key={`${account}:${workspace}:${userId}`} className="ph-no-capture ph-no-record px-sm pt-xl">
    <SectionLabel variant="navigation" className="mb-sm px-md">{HISTORY_COPY.heading}</SectionLabel>
    {account ? <RecentFleetList workspace={workspace} onNavigate={onNavigate} /> : null}
    {current ? <SidebarChats history={current} onNavigate={onNavigate} showEmpty={!account} /> : <StandaloneDraftHistory workspace={workspace} subject={userId} onNavigate={onNavigate} />}
  </div>;
}

function SidebarChats({history, onNavigate, showEmpty}: {history: DraftSidebarState; onNavigate: () => void; showEmpty: boolean}) {
  const cards = history.cards.filter(card => !card.linkedFleet);
  return <>
      <DraftChatList history={history} cards={cards} onNavigate={onNavigate} />
      {!cards.length && (showEmpty || history.status === HISTORY_STATUS.unavailable) ? <p className="px-md py-sm text-label text-muted-foreground">{history.status === HISTORY_STATUS.unavailable ? HISTORY_COPY.unavailable
        : history.status === HISTORY_STATUS.loading ? HISTORY_COPY.loading : HISTORY_COPY.empty}</p> : null}
      {history.more ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
  </>;
}
