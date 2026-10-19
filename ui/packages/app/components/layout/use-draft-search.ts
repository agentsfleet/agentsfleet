"use client";

import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { DraftSearch, type DraftSearchView } from "@/lib/api/draft-search";
import { DRAFT_SEARCH_STATUS as STATUS } from "@/lib/api/draft-vocabulary";
import type { SidebarDraft } from "@/lib/api/draft-summary";
const SEARCH_DELAY_MS = 250;

export function useDraftSearch(workspace: string, query: string, history: readonly SidebarDraft[], version: string | null) {
  const search = useMemo(() => new DraftSearch(workspace), [workspace]);
  const previousQuery = useRef(query);
  const snapshot = useSyncExternalStore(search.subscribe, search.getSnapshot, search.getSnapshot);
  useEffect(() => {
    const refresh = previousQuery.current === query && search.getSnapshot().query === query.trim();
    previousQuery.current = query;
    // A changed chat can change result order; begin again instead of retaining its old cursor or labels.
    if (refresh) {void search.search(query); return () => {search.cancel();};}
    const timer = setTimeout(() => {void search.search(query);}, SEARCH_DELAY_MS);
    return () => {clearTimeout(timer); search.cancel();};
  }, [search, query, history, version]);
  const view: DraftSearchView = snapshot.query === query.trim() ? snapshot
    : {query: query.trim(), status: STATUS.loading, items: [], next_cursor: null};
  return {view, more: search.more, remove: search.remove, retry: () => {void search.search(query);}};
}
