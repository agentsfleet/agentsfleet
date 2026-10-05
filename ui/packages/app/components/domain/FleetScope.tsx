"use client";

import { createContext, useContext, useMemo, type ReactNode } from "react";

// The workspace and fleet a thread reads, for the reads a row makes on its own
// (a tool call's "show all"). The thread provides it; a row outside a thread
// has none, and offers nothing that needs it.

export type FleetScope = { workspaceId: string; fleetId: string };

const FleetScopeContext = createContext<FleetScope | null>(null);

export function FleetScopeProvider({ workspaceId, fleetId, children }: FleetScope & { children: ReactNode }) {
  const scope = useMemo(() => ({ workspaceId, fleetId }), [workspaceId, fleetId]);
  return <FleetScopeContext value={scope}>{children}</FleetScopeContext>;
}

/** Null outside a thread: nothing there can be read in full. */
export function useFleetScope(): FleetScope | null {
  return useContext(FleetScopeContext);
}
