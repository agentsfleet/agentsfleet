"use client";

import { useCallback, useMemo, useSyncExternalStore } from "react";

import {
  NO_PENDING_SENDS,
  PENDING_SEND_STATE,
  beginPendingSend,
  dismissPendingSend,
  failPendingSend,
  findPendingSendByText,
  getPendingSends,
  settlePendingSend,
  subscribePendingSends,
  type PendingSend,
  type PendingSendState,
} from "@/lib/streaming/pending-sends";

export { PENDING_SEND_STATE, type PendingSend, type PendingSendState };

/** The ledger, bound to one fleet: what the delivery chain writes and the notice reads. */
export type FleetPendingSends = {
  pending: readonly PendingSend[];
  begin: (send: Omit<PendingSend, "state">) => void;
  settle: (operationId: string) => void;
  fail: (operationId: string, state: Exclude<PendingSendState, typeof PENDING_SEND_STATE.SENDING>) => void;
  dismiss: (operationId: string) => void;
  byText: (text: string) => PendingSend | undefined;
};

// The React boundary over `lib/streaming/pending-sends`. The store keeps one
// array per fleet by reference until something changes, so the snapshot is
// stable between writes and the server snapshot is the shared empty list.
export function useFleetPendingSends(workspaceId: string, fleetId: string): FleetPendingSends {
  const subscribe = useCallback(
    (listener: () => void) => subscribePendingSends(workspaceId, fleetId, listener),
    [workspaceId, fleetId],
  );
  const getSnapshot = useCallback(() => getPendingSends(workspaceId, fleetId), [workspaceId, fleetId]);
  const pending = useSyncExternalStore(subscribe, getSnapshot, () => NO_PENDING_SENDS);
  const writers = useMemo(
    () => ({
      begin: (send: Omit<PendingSend, "state">) => beginPendingSend(workspaceId, fleetId, send),
      settle: (operationId: string) => settlePendingSend(workspaceId, fleetId, operationId),
      fail: (operationId: string, state: Exclude<PendingSendState, typeof PENDING_SEND_STATE.SENDING>) =>
        failPendingSend(workspaceId, fleetId, operationId, state),
      dismiss: (operationId: string) => dismissPendingSend(workspaceId, fleetId, operationId),
      byText: (text: string) => findPendingSendByText(workspaceId, fleetId, text),
    }),
    [workspaceId, fleetId],
  );
  return useMemo(() => ({ pending, ...writers }), [pending, writers]);
}
