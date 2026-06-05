"use client";

import { useState, useTransition } from "react";
import {
  Badge,
  type BadgeVariant,
  Button,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import {
  RUNNER_SORTS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type RunnerListResponse,
  type RunnerListItem,
  type RunnerLiveness,
  type RunnerSort,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction } from "../actions";
import AddRunnerDialog from "./AddRunnerDialog";

const SORT_LABELS: Record<RunnerSort, string> = {
  "-created_at": "Newest first",
  created_at: "Oldest first",
  host_id: "Host A–Z",
  "-host_id": "Host Z–A",
};

// Derived liveness → badge colour. registered = not yet connected (amber);
// online = idle + reachable (green); busy = holding a live lease (cyan); offline
// = heartbeat lapsed (muted default).
const LIVENESS_VARIANT: Record<RunnerLiveness, BadgeVariant> = {
  registered: "amber",
  online: "green",
  busy: "cyan",
  offline: "default",
};

function fmt(ms: number): string {
  return new Date(ms).toLocaleString();
}

export default function RunnerList({ initial }: { initial: RunnerListResponse }) {
  const [pending, startTransition] = useTransition();
  const [items, setItems] = useState<RunnerListItem[]>(initial.items);
  const [total, setTotal] = useState(initial.total);
  const [page, setPage] = useState(initial.page);
  const [sort, setSort] = useState<RunnerSort>(DEFAULT_SORT);
  const [error, setError] = useState<string | null>(null);

  const lastPage = Math.max(1, Math.ceil(total / DEFAULT_PAGE_SIZE));

  function apply(data: RunnerListResponse, nextSort: RunnerSort) {
    setItems(data.items);
    setTotal(data.total);
    setPage(data.page);
    setSort(nextSort);
  }

  function loadPage(next: { page: number; sort?: RunnerSort }, retried = false) {
    const nextPage = next.page;
    const nextSort = next.sort ?? sort;
    startTransition(async () => {
      const r = await listRunnersAction({ page: nextPage, page_size: DEFAULT_PAGE_SIZE, sort: nextSort });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load runners" }));
        if (r.errorCode === "UZ-REQ-001" && !retried) loadPage({ page: 1, sort: DEFAULT_SORT }, true);
        return;
      }
      setError(null);
      apply(r.data, nextSort);
    });
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Select value={sort} onValueChange={(v) => loadPage({ sort: v as RunnerSort, page: 1 })}>
          <SelectTrigger className="w-44" aria-label="Sort runners">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {RUNNER_SORTS.map((s) => (
              <SelectItem key={s} value={s}>
                {SORT_LABELS[s]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <AddRunnerDialog onCreated={() => loadPage({ page: 1, sort: DEFAULT_SORT })} />
      </div>

      {items.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No runners enrolled yet. Add one to give the fleet a host to lease work to.
        </p>
      ) : (
        <div className="divide-y rounded-md border">
          {items.map((r) => (
            <Row key={r.id} r={r} />
          ))}
        </div>
      )}

      {error ? <p className="text-sm text-destructive">{error}</p> : null}

      {lastPage > 1 ? (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            Page {page} of {lastPage} · {total} runners
          </span>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={pending || page <= 1} onClick={() => loadPage({ page: page - 1 })}>
              Previous
            </Button>
            <Button type="button" variant="ghost" size="sm" disabled={pending || page >= lastPage} onClick={() => loadPage({ page: page + 1 })}>
              Next
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Row({ r }: { r: RunnerListItem }) {
  return (
    <div className="flex items-center justify-between gap-3 p-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate font-mono text-sm">{r.host_id}</span>
          <Badge variant={LIVENESS_VARIANT[r.liveness]}>{r.liveness}</Badge>
          <Badge variant="default">{r.sandbox_tier}</Badge>
        </div>
        <div className="font-mono text-xs tabular-nums text-muted-foreground">
          enrolled {fmt(r.created_at)} ·{" "}
          {r.last_seen_at > 0 ? `last seen ${fmt(r.last_seen_at)}` : "never connected"}
          {r.labels.length > 0 ? ` · ${r.labels.join(", ")}` : ""}
        </div>
      </div>
    </div>
  );
}
