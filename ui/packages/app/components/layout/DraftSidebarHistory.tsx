"use client";

import { useContext, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ConfirmDialog, Input, SectionLabel } from "@agentsfleet/design-system";
import { SearchIcon } from "lucide-react";
import { useCurrentUser } from "@/lib/auth/client";
import { DRAFT_LIST_STATUS as HISTORY_STATUS } from "@/lib/api/draft-list-types";
import type { SidebarDraft } from "@/lib/api/draft-summary";
import { workspacePath } from "@/lib/workspace-routes";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { DraftSidebarItem } from "./DraftSidebarItem";
import { HistoryContext, UpdateContext, type DraftSidebarState } from "./draft-sidebar-state";
import { useDeleteDraft } from "./use-delete-draft";

export { DraftSidebarProvider, useDraftSidebarBridge, useFreshFleetChat } from "./draft-sidebar-state";

const HISTORY_COPY = {
  heading: "Fleet setup chats", search: "Search loaded chats", empty: "Your fleet setup conversations will appear here.",
  loading: "Loading setup chats…", unavailable: "Chat history is unavailable.", noMatches: "No matching chats",
} as const;

export function DraftSidebarHistory({workspace, onNavigate}: {workspace: string | null; onNavigate: () => void}) {
  const history = useContext(HistoryContext);
  const {userId} = useCurrentUser();
  if (!history || history.subject !== userId || history.workspace !== workspace || !workspace) return null;
  return <SidebarChats key={`${workspace}:${userId}`} history={history} onNavigate={onNavigate} />;
}

function SidebarChats({history, onNavigate}: {history: DraftSidebarState; onNavigate: () => void}) {
  const update = useContext(UpdateContext);
  const router = useRouter();
  const search = useSearchParams();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SidebarDraft | null>(null);
  const onRemoved = (id: string): void => {
    update((value) => value?.workspace === history.workspace && value.subject === history.subject
      ? {...value, cards: value.cards.filter((card) => card.id !== id), current: value.current === id ? null : value.current} : value);
    if (id === (search.get("draft") ?? history.current)) router.replace(workspacePath(history.workspace, "fleets/new"));
    setSelected(null);
  };
  const deletion = useDeleteDraft(history.workspace, history.remove, onRemoved);
  const filtered = history.cards.filter((card) => card.title.toLowerCase().includes(query.trim().toLowerCase()));
  const active = search.get("draft") ?? history.current;
  return <div className="ph-no-capture ph-no-record px-3 pt-xl">
    <SectionLabel className="px-md pb-md">{HISTORY_COPY.heading}</SectionLabel>
    {history.cards.length > 0 ? <div className="relative mb-sm">
      <SearchIcon size={14} className="pointer-events-none absolute left-3 top-3 text-muted-foreground" aria-hidden="true" />
      <Input aria-label={HISTORY_COPY.search} placeholder={HISTORY_COPY.search} value={query} onChange={(event) => setQuery(event.target.value)} className="pl-9" />
    </div> : null}
    {filtered.map((card) => <DraftSidebarItem key={card.id} card={card} workspace={history.workspace} active={card.id === active}
      disabled={history.locked || deletion.pending} onNavigate={onNavigate} onDelete={() => {deletion.clearError(); setSelected(card);}} />)}
    {!filtered.length ? <p className="px-md py-sm text-label text-muted-foreground">{history.status === HISTORY_STATUS.unavailable ? HISTORY_COPY.unavailable
      : history.status === HISTORY_STATUS.loading ? HISTORY_COPY.loading
      : history.cards.length > 0 ? HISTORY_COPY.noMatches : HISTORY_COPY.empty}</p> : null}
    {history.more ? <DraftLoadMore className="px-md pt-sm" copy={LOAD_MORE_COPY.chats} onLoad={history.more} /> : null}
    {history.more && query.trim() ? <p className="px-md py-sm text-eyebrow text-muted-foreground">Search covers loaded chat titles. Load more chats to include older conversations.</p> : null}
    <ConfirmDialog open={selected !== null} onOpenChange={(open) => {if (!open) setSelected(null);}} title="Delete setup chat?" confirmLabel="Delete chat" intent="destructive"
      description={`Delete “${selected?.title ?? "this chat"}” and its private conversation? Saved fleets and published library versions stay available.`}
      onConfirm={selected ? () => deletion.remove(selected.id) : undefined} errorMessage={deletion.error} />
  </div>;
}
