"use client";

import {useEffect, useState} from "react";
import type {z} from "zod";
import {RecentCollection} from "./recent-collection";

export function useRecentPage<T extends {id: string}>(path: string | null, schema: z.ZodType<{items: T[]; next_cursor: string | null}>) {
  const [collection, setCollection] = useState<RecentCollection<T> | null>(null);
  useEffect(() => {
    if (!path) return;
    const current = new RecentCollection(path, schema);
    setCollection(current);
    void current.load();
    return () => current.dispose();
  }, [path, schema]);
  return collection?.path === path ? collection : null;
}
