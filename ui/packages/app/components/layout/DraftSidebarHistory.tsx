"use client";

import { useContext } from "react";
import { SectionLabel } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { DraftChatList } from "./DraftChatList";
import { HistoryContext, type DraftSidebarState } from "./draft-sidebar-state";

const HISTORY_COPY = {
  heading: "Recent setup", empty: "Your setup chats will appear here.",
  loading: "Loading setup chats…", unavailable: "Chat history is unavailable.",
} as const;

export function DraftSidebarHistory({workspace, onNavigate}: {workspace: string | null; onNavigate: () => void}) {
  const history = useContext(HistoryContext);
  const {userId} = useCurrentUser();
  if (!history || history.subject !== userId || history.workspace !== workspace || !workspace) return null;
  return <SidebarChats key={`${workspace}:${userId}`} history={history} onNavigate={onNavigate} />;
}

function SidebarChats({history, onNavigate}: {history: DraftSidebarState; onNavigate: () => void}) {
  return <div className="ph-no-capture ph-no-record px-sm pt-xl">
    <SectionLabel variant="navigation" className="mb-sm px-md">{HISTORY_COPY.heading}</SectionLabel>
      <DraftChatList history={history} cards={history.cards} onNavigate={onNavigate} />
      {!history.cards.length ? <p className="px-md py-sm text-label text-muted-foreground">{history.status === HISTORY_STATUS.unavailable ? HISTORY_COPY.unavailable
        : history.status === HISTORY_STATUS.loading ? HISTORY_COPY.loading : HISTORY_COPY.empty}</p> : null}
      {history.more ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
  </div>;
}
