"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type Dispatch, type SetStateAction, type ReactNode } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Input, NavItem, SectionLabel } from "@agentsfleet/design-system";
import { MessageSquareIcon, SearchIcon } from "lucide-react";
import { useCurrentUser } from "@/lib/auth/client";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { DraftCard } from "@/lib/api/draft-types";
import { workspacePath } from "@/lib/workspace-routes";
import { DRAFT_LIST, DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";

type History = { workspace: string; subject: string; cards: readonly Pick<DraftCard, "id" | "title">[]; fresh: (() => void) | null; locked: boolean; current: string | null;
  more: (() => Promise<void>) | null; status: HistoryStatus };
type HistoryStatus = typeof HISTORY_STATUS[keyof typeof HISTORY_STATUS];
const HISTORY_COPY = {
  heading: "Fleet setup chats", search: "Search loaded chats", empty: "Your fleet setup conversations will appear here.",
  loading: "Loading setup chats…", unavailable: "Chat history is unavailable.", noMatches: "No matching chats",
} as const;
const HistoryContext = createContext<History | null>(null);
const UpdateContext = createContext<Dispatch<SetStateAction<History | null>>>(() => {});

export function DraftSidebarProvider({ children }: { children: ReactNode }) {
  const [history, setHistory] = useState<History | null>(null);
  return <UpdateContext value={setHistory}><HistoryContext value={history}>{children}</HistoryContext></UpdateContext>;
}

export function useDraftSidebarBridge(client: DraftClient, view: DraftView, workspace: string, subject: string, running: boolean) {
  const update = useContext(UpdateContext);
  const fresh = useCallback(() => client.fresh(), [client]);
  const loadChats = useCallback(() => client.loadMore(DRAFT_LIST.chats), [client]);
  const current = view.draft?.id ?? null;
  const title = view.draft?.title ?? "";
  const source = view.cards;
  const inaccessible = view.inaccessible;
  const locked = view.busy || view.pending || inaccessible || running || view.listStatus !== HISTORY_STATUS.ready;
  const more = view.moreChats && !inaccessible ? loadChats : null;
  const cards = useMemo(() => inaccessible ? [] : [
    ...(current ? [{ id: current, title }] : []),
    ...source.filter((card) => card.id !== current).map(({ id, title }) => ({ id, title })),
  ], [source, current, title, inaccessible]);
  const status = view.listStatus;
  useEffect(() => {
    update((previous) => {
      if (previous?.workspace === workspace && previous.subject === subject && previous.fresh === fresh && previous.locked === locked && previous.current === current && previous.more === more && previous.status === status
        && previous.cards.length === cards.length && previous.cards.every((card, index) => card.id === cards[index]?.id && card.title === cards[index].title)) return previous;
      return { workspace, subject, cards, fresh, locked, current, more, status };
    });
  }, [update, workspace, subject, cards, fresh, locked, current, more, status]);
  useEffect(() => () => update((current) => current && current.status !== HISTORY_STATUS.loading
    ? { ...current, fresh: null, current: null, more: null } : null), [update, client]);
}

export function useFreshFleetChat(workspace: string | null) {
  const history = useContext(HistoryContext);
  const { userId } = useCurrentUser();
  return () => { if (history?.workspace === workspace && history.subject === userId && !history.locked) history.fresh?.(); };
}

export function DraftSidebarHistory({ workspace, onNavigate }: { workspace: string | null; onNavigate: () => void }) {
  const history = useContext(HistoryContext);
  const { userId } = useCurrentUser();
  const [query, setQuery] = useState("");
  const search = useSearchParams();
  useEffect(() => setQuery(""), [workspace, userId]);
  if (!history || history.subject !== userId || history.workspace !== workspace || !workspace) return null;
  const filtered = history.cards.filter((card) => card.title.toLowerCase().includes(query.trim().toLowerCase()));
  const active = search.get("draft") ?? history.current;
  return <div className="ph-no-capture ph-no-record px-3 pt-xl">
    <SectionLabel className="px-md pb-md">{HISTORY_COPY.heading}</SectionLabel>
    {history.cards.length > 0 ? <div className="relative mb-sm">
      <SearchIcon size={14} className="pointer-events-none absolute left-3 top-3 text-muted-foreground" aria-hidden="true" />
      <Input aria-label={HISTORY_COPY.search} placeholder={HISTORY_COPY.search} value={query} onChange={(event) => setQuery(event.target.value)} className="pl-9" />
    </div> : null}
    {filtered.map((card) => <NavItem key={card.id} asChild active={card.id === active} onClick={onNavigate}>
      <Link href={`${workspacePath(workspace, "fleets/new")}?draft=${encodeURIComponent(card.id)}`} prefetch={false}>
        <MessageSquareIcon size={14} className="shrink-0" /><span className="truncate">{card.title}</span>
      </Link>
    </NavItem>)}
    {!filtered.length ? <p className="px-md py-sm text-label text-muted-foreground">{history.status === HISTORY_STATUS.unavailable ? HISTORY_COPY.unavailable
      : history.status === HISTORY_STATUS.loading ? HISTORY_COPY.loading
      : history.cards.length > 0 ? HISTORY_COPY.noMatches : HISTORY_COPY.empty}</p> : null}
    {history.more ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
  </div>;
}
