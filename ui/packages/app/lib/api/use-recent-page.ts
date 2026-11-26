"use client";

import {useEffect, useState} from "react";
import type {z} from "zod";
import {RecentCollection} from "./recent-collection";
import {fleetsPath} from "./fleet-paths";

const collectionChanges = new EventTarget();

export function refreshRecentFleets(workspace: string): void {
  collectionChanges.dispatchEvent(new Event(`/live${fleetsPath(workspace)}`));
}

export function useRecentPage<T extends {id: string}>(path: string | null, schema: z.ZodType<{items: T[]; next_cursor: string | null}>) {
  const [collection, setCollection] = useState<RecentCollection<T> | null>(null);
  useEffect(() => {
    if (!path) return;
    const current = new RecentCollection(path, schema);
    const refresh = () => current.refresh(0);
    collectionChanges.addEventListener(path, refresh);
    setCollection(current);
    void current.load();
    return () => {
      collectionChanges.removeEventListener(path, refresh);
      current.dispose();
    };
  }, [path, schema]);
  return collection?.path === path ? collection : null;
}
