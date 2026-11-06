"use client";

import { useCallback, useEffect, useState } from "react";
import type { TaskRead, TaskReadFailure } from "@/lib/api/tasks";

export type TaskResource<T> =
  | {kind: "loading"; previous: T | null}
  | {kind: "ready"; value: T}
  | {kind: "failed"; previous: T | null; reason: TaskReadFailure};

export function confirmed<T>(resource: TaskResource<T>): T | null {
  return resource.kind === "ready" ? resource.value : resource.previous;
}

/** The caller keys this mounted resource by account, workspace and task scope. */
export function useTaskRead<T>(read: (signal: AbortSignal) => Promise<TaskRead<T>>, revision: number) {
  const [resource, setResource] = useState<TaskResource<T>>({kind: "loading", previous: null});
  const [retryRevision, setRetryRevision] = useState(0);
  const retry = useCallback(() => setRetryRevision(value => value + 1), []);
  useEffect(() => {
    const current = new AbortController();
    setResource(previous => ({kind: "loading", previous: confirmed(previous)}));
    let recovery: ReturnType<typeof setTimeout> | undefined;
    void read(current.signal).then(result => {
      if (current.signal.aborted) return;
      if (result.kind === "failed" && result.reason !== "access" && result.reason !== "missing") {
        recovery = setTimeout(retry, 5000);
      }
      setResource(previous => result.kind === "ready" ? result : {
        kind: "failed", reason: result.reason,
        previous: result.reason === "access" || result.reason === "missing" ? null : confirmed(previous),
      });
    });
    return () => {current.abort(); clearTimeout(recovery);};
  }, [read, revision, retryRevision, retry]);
  return {resource, retry};
}
