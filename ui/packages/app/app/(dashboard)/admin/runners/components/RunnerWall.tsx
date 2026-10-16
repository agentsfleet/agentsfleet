"use client";

import { useState, useTransition } from "react";
import { ServerIcon } from "lucide-react";
import { Alert, EmptyState, Pagination, PAGINATION_KIND } from "@agentsfleet/design-system";
import type { RunnerListItem } from "@/lib/api/runners";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { listRunnersAction } from "../actions";
import RunnerTile from "./RunnerTile";

const PAGE_SIZE = 25;
const LOAD_ACTION = "load runners";
type RunnerPage = { items: RunnerListItem[]; cursor: string | null };
type WallState = { source: RunnerListItem[]; cursor: string | null; past: RunnerPage[]; current: RunnerPage; future: RunnerPage[]; error: string | null };

function initialState(source: RunnerListItem[], cursor: string | null): WallState {
  return { source, cursor, past: [], current: { items: source, cursor }, future: [], error: null };
}

export default function RunnerWall({ initialRunners, initialCursor }: {
  initialRunners: RunnerListItem[];
  initialCursor: string | null;
}) {
  const [state, setState] = useState(() => initialState(initialRunners, initialCursor));
  const [pending, startTransition] = useTransition();
  if (state.source !== initialRunners || state.cursor !== initialCursor) {
    setState(initialState(initialRunners, initialCursor));
  }
  const page = state.current;
  const pageNumber = state.past.length + 1;
  const previous = state.past.at(-1);
  const cachedNext = state.future[0];
  const back = previous ? () => setState((current) => ({...current, current: previous,
    past: current.past.slice(0, -1), future: [current.current, ...current.future], error: null})) : undefined;
  const cursor = page.cursor;
  const forward = cachedNext ? () => setState((current) => ({...current, current: cachedNext,
    past: [...current.past, current.current], future: current.future.slice(1), error: null}))
    : cursor ? () => loadNext(cursor) : undefined;
  const navigate = back && forward ? (number: number) => number < pageNumber ? back() : forward() : back ?? forward;

  function loadNext(cursor: string) {
    startTransition(async () => {
      try {
        const result = await listRunnersAction({ starting_after: cursor, limit: PAGE_SIZE });
        setState((current) => {
          // Refreshing either the rows or cursor invalidates outstanding reads.
          if (current.source !== state.source || current.cursor !== state.cursor) return current;
          if (!result.ok) return { ...current, error: presentErrorString({action: LOAD_ACTION, message: result.error, errorCode: result.errorCode}) };
          const next = { items: result.data.items, cursor: result.data.next_cursor === cursor ? null : result.data.next_cursor };
          return { ...current, current: next, past: [...current.past, current.current], future: [], error: null };
        });
      } catch (cause) {
        setState((current) => current.source !== state.source || current.cursor !== state.cursor ? current
          : { ...current, error: unexpectedErrorMessage(cause, LOAD_ACTION) });
      }
    });
  }
  return <div className="space-y-xl">
    {page.items.length ? <div className="grid grid-cols-1 gap-xl sm:grid-cols-2 xl:grid-cols-3">
      {page.items.map((runner) => <RunnerTile key={runner.id} runner={runner} />)}
    </div> : <EmptyState icon={<ServerIcon size={28} />} title="No runners enrolled"
      description="Enroll a host to run fleets and it appears here." />}
    {state.error ? <Alert variant="destructive">{state.error}</Alert> : null}
    {navigate ? <Pagination kind={PAGINATION_KIND.page} page={pageNumber}
      pageSize={PAGE_SIZE} hasNext={Boolean(forward)} isLoading={pending} onPageChange={navigate} /> : null}
  </div>;
}
