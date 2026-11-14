"use client";

import type { ReactNode } from "react";

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useSyncExternalStore,
} from "react";
import {
  EMPTY_TILE,
  EMPTY_WORKSPACE,
  WorkspaceStore,
  type Listener,
  type TileCounters,
  type WorkspaceStreamSnapshot,
  type WorkspaceTileSnapshot,
} from "@/lib/streaming/workspace-store";

// Re-exported so every existing caller keeps importing its snapshot type from
// the hook it uses, rather than reaching past it into the store.
export type { TileCounters, WorkspaceStreamSnapshot, WorkspaceTileSnapshot };

const WorkspaceStreamContext = createContext<WorkspaceStore | null>(null);

export function WorkspaceStreamProvider({
  workspaceId,
  fleetIds,
  children,
}: {
  workspaceId: string;
  fleetIds: string[];
  children: ReactNode;
}) {
  const store = useMemo(() => new WorkspaceStore(workspaceId), [workspaceId]);
  const ids = useMemo(() => [...new Set(fleetIds)].sort(), [fleetIds]);

  useEffect(() => {
    return store.connect(ids);
  }, [store, ids]);

  return React.createElement(WorkspaceStreamContext.Provider, { value: store }, children);
}

export function useWorkspaceFleetStream(fleetId: string): WorkspaceTileSnapshot {
  const store = useContext(WorkspaceStreamContext);
  const subscribe = useCallback(
    (listener: Listener) => store?.subscribe(fleetId, listener) ?? (() => {}),
    [store, fleetId],
  );
  const getSnapshot = useCallback(() => store?.snapshot(fleetId) ?? EMPTY_TILE, [store, fleetId]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/**
 * The wall's own view of the one workspace stream: whether it is connected
 * and greeted. One subscription for the whole wall, independent of tile
 * count. The tile footers need nothing from here: every frame carries the
 * fleet's counters, so no tile ever has to ask the server for them.
 */
export function useWorkspaceStream(): WorkspaceStreamSnapshot {
  const store = useContext(WorkspaceStreamContext);
  const subscribe = useCallback(
    (listener: Listener) => store?.subscribeWorkspace(listener) ?? (() => {}),
    [store],
  );
  const getSnapshot = useCallback(
    () => store?.workspaceSnapshot() ?? EMPTY_WORKSPACE,
    [store],
  );
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
