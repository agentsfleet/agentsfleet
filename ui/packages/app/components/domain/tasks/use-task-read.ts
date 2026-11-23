"use client";

import type { DurableTaskRead, DurableTaskReadFailure } from "@/lib/api/tasks";
import { useCallback, useEffect, useState } from "react";
import { recoverDurableTaskRead } from "./task-read-recovery";

export type DurableTaskResource<T> =
  | {kind: "loading"; previous: T | null; accessDenied?: true}
  | {kind: "ready"; value: T}
  | {kind: "failed"; previous: T | null; reason: DurableTaskReadFailure; accessDenied?: true};

/** The caller keys this mounted resource by account, workspace and task scope. */
export function useDurableTaskRead<T>(read: (signal: AbortSignal) => Promise<DurableTaskRead<T>>, revision: number) {
  const [resource, setResource] = useState<DurableTaskResource<T>>({kind: "loading", previous: null});
  const [retryRevision, setRetryRevision] = useState(0);
  const retry = useCallback(() => setRetryRevision(value => value + 1), []);
  useEffect(() => {
    const current = new AbortController();
    setResource(previous => ({kind: "loading", previous: confirmed(previous), ...(accessLost(previous) ? {accessDenied: true as const} : {})}));
    void recoverDurableTaskRead(read, current.signal, result => {
      setResource(previous => result.kind === "ready" ? result : {
        kind: "failed", reason: result.reason,
        previous: result.reason === "access" || result.reason === "missing" ? null : confirmed(previous),
        ...(result.reason === "access" || result.reason === "missing" || accessLost(previous) ? {accessDenied: true as const} : {}),
      });
    });
    return () => current.abort();
  }, [read, revision, retryRevision, retry]);
  return {resource, retry};
}

export function confirmed<T>(resource: DurableTaskResource<T>): T | null {
  return resource.kind === "ready" ? resource.value : resource.previous;
}

/** A retry does not restore authority; only a confirmed authorized response does. */
export function accessLost<T>(resource: DurableTaskResource<T>): boolean {
  return resource.kind !== "ready" && (resource.accessDenied === true || resource.kind === "failed" && (resource.reason === "access" || resource.reason === "missing"));
}
