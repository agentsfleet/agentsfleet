"use client";

import type { DurableTaskDetail } from "@/lib/api/tasks";
import type { DurableTaskResource } from "./use-task-read";
import type { DurableTaskReference } from "./task-references";
import { useCallback, useEffect, useMemo, useState } from "react";
import { confirmed } from "./use-task-read";
import { loadDurableTaskDetails } from "./task-detail-reader";

export type DurableTaskDetails = ReadonlyMap<string, DurableTaskResource<DurableTaskDetail>>;


/** Parent is keyed by account/workspace/fleet; removed tasks lose their saved data. */
export function useDurableTaskDetails(workspace: string, fleet: string, tasks: readonly DurableTaskReference[]) {
  const [details, setDetails] = useState<DurableTaskDetails>(new Map());
  const [revision, setRevision] = useState(0);
  const retry = useCallback(() => setRevision(value => value + 1), []);
  useEffect(() => {
    const current = new AbortController();
    setDetails(previous => new Map(tasks.map(task => [task.id, {kind: "loading" as const, previous: retainedDetail(previous, task.id)}])));
    void loadDurableTaskDetails(workspace, fleet, tasks, current.signal, (task, result) => {
      // The reader fences every observation before invoking this synchronous callback.
      setDetails(previous => {
        const saved = retainedDetail(previous, task.id);
        const resource: DurableTaskResource<DurableTaskDetail> = result === null ? {kind: "loading", previous: saved}
          : result.kind === "ready" ? result : {kind: "failed", reason: result.reason,
            previous: result.reason === "access" || result.reason === "missing" ? null : saved};
        return new Map(previous).set(task.id, resource);
      });
    });
    return () => current.abort();
  }, [workspace, fleet, tasks, revision]);
  const scoped = useMemo(() => new Map(tasks.flatMap(task => {
    const resource = details.get(task.id);
    return resource ? [[task.id, resource] as const] : [];
  })), [details, tasks]);
  return {details: scoped, retry};
}

function retainedDetail(details: DurableTaskDetails, id: string): DurableTaskDetail | null {
  const resource = details.get(id);
  return resource ? confirmed(resource) : null;
}
