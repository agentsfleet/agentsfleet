"use client";

import { useCallback, useMemo, useSyncExternalStore } from "react";

import {
  NO_PENDING_SENDS,
  PENDING_SEND_STATE,
  beginPendingSend,
  dismissPendingSend,
  failPendingSend,
  findPendingSend,
  getPendingSends,
  settlePendingSend,
  subscribePendingSends,
  type LedgerScope,
  type PendingSend,
  type PendingSendOutcome,
} from "@/lib/streaming/pending-sends";

export { PENDING_SEND_STATE, type LedgerScope, type PendingSend, type PendingSendOutcome };

/** The ledger's writers, bound to one user's fleet. Stable for a scope, so a
 * callback that depends on them is not rebuilt by every ledger write. */
export type PendingSendWriters = {
  begin: (send: Omit<PendingSend, "state">) => void;
  settle: (operationId: string) => void;
  fail: (operationId: string, state: PendingSendOutcome) => void;
  dismiss: (operationId: string) => void;
  /** One entry, read from the ledger rather than from the last render. */
  find: (operationId: string) => PendingSend | undefined;
  /** Every entry, tombstones included, read from the ledger now. */
  list: () => readonly PendingSend[];
};

export type FleetPendingSends = {
  pending: readonly PendingSend[];
  writers: PendingSendWriters;
};

// The React boundary over `lib/streaming/pending-sends`. The store keeps one
// array per scope by reference until something changes, so the snapshot is
// stable between writes and the server snapshot is the shared empty list.
export function useFleetPendingSends({ subject, workspaceId, fleetId }: LedgerScope): FleetPendingSends {
  const scope = useMemo(() => ({ subject, workspaceId, fleetId }), [subject, workspaceId, fleetId]);
  const subscribe = useCallback((listener: () => void) => subscribePendingSends(scope, listener), [scope]);
  const getSnapshot = useCallback(() => getPendingSends(scope), [scope]);
  const pending = useSyncExternalStore(subscribe, getSnapshot, () => NO_PENDING_SENDS);
  const writers = useMemo<PendingSendWriters>(
    () => ({
      begin: (send) => beginPendingSend(scope, send),
      settle: (operationId) => settlePendingSend(scope, operationId),
      fail: (operationId, state) => failPendingSend(scope, operationId, state),
      dismiss: (operationId) => dismissPendingSend(scope, operationId),
      find: (operationId) => findPendingSend(scope, operationId),
      list: () => getPendingSends(scope),
    }),
    [scope],
  );
  return useMemo(() => ({ pending, writers }), [pending, writers]);
}
