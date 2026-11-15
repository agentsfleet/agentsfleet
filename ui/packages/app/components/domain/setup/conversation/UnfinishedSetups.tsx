"use client";

import { useContext } from "react";
import { Button, Section, Skeleton } from "@agentsfleet/design-system";
import { useCurrentUser } from "@/lib/auth/client";
import { DRAFT_LIST_STATUS } from "@/lib/api/draft-list-types";
import { CHAT_ACTIVITY } from "@/lib/api/draft-summary";
import { HistoryContext } from "@/components/layout/draft-sidebar-state";
import { DraftChatList } from "@/components/layout/DraftChatList";
import { DraftLoadMore, LOAD_MORE_COPY } from "./DraftLoadMore";

const stayOnPage = (): void => undefined;

export function UnfinishedSetups({workspace, onReload}: {workspace: string; onReload: () => Promise<void>}) {
  const history = useContext(HistoryContext);
  const {userId} = useCurrentUser();
  if (!history || history.workspace !== workspace || history.subject !== userId) return null;
  const cards = history.cards.filter(card => card.activity !== CHAT_ACTIVITY.saved);
  if (history.status === DRAFT_LIST_STATUS.ready && !cards.length && !history.more) return null;
  return <Section aria-label="Unfinished setups" className="mt-xl gap-md border-t border-border pt-xl">
    <div><h2 className="text-body font-semibold">Unfinished setups</h2>
      <p className="mt-xs text-label text-muted-foreground">Pick up where you left off. Only you can see these.</p></div>
    {history.status === DRAFT_LIST_STATUS.loading ? <Skeleton className="h-12 w-full" aria-label="Loading unfinished setups" /> : null}
    {history.status === DRAFT_LIST_STATUS.unavailable ? <output className="text-label text-muted-foreground">
      Unfinished setups could not be loaded. <Button size="sm" variant="link" onClick={() => {void onReload().catch(stayOnPage);}}>Try again</Button>
    </output> : null}
    <DraftChatList history={history} cards={cards} onNavigate={stayOnPage} />
    {history.more ? <DraftLoadMore copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
  </Section>;
}
