"use client";

import { useCallback, useSyncExternalStore } from "react";
import type { AppendMessage } from "@assistant-ui/react";

// The delivery-failure surface for one fleet's composer.
//
// There is deliberately no browser-side hold here. Ordering belongs to the
// fleet's own event stream, which serialises every producer — webhook, cron,
// continuation and operator alike. A second queue in the browser could only
// disagree with it, and while it existed a live feed that was merely down
// turned the console read-only: every submission sat marked as queued and
// none of them was ever sent.
//
// State lives at module scope, keyed by fleet, so a failure survives the
// component unmounting and remounting across a navigation.

const FAILURE_REGISTRY = new Map<string, FailedDelivery>();
const FAILURE_LISTENERS = new Map<string, Set<() => void>>();

export type DeliveryFailureKind = "send" | "session";

export type FailedDelivery = {
  message: AppendMessage;
  kind: DeliveryFailureKind;
};

export function useFleetDeliveryFailure(fleetId: string): {
  failedDelivery: FailedDelivery | null;
  setFailedDelivery: (failure: FailedDelivery) => void;
  clearFailedDelivery: () => void;
} {
  const subscribe = useCallback((listener: () => void) => {
    const listeners = FAILURE_LISTENERS.get(fleetId) ?? new Set<() => void>();
    listeners.add(listener);
    FAILURE_LISTENERS.set(fleetId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) FAILURE_LISTENERS.delete(fleetId);
    };
  }, [fleetId]);
  const getSnapshot = useCallback(
    () => FAILURE_REGISTRY.get(fleetId) ?? null,
    [fleetId],
  );
  const failedDelivery = useSyncExternalStore(subscribe, getSnapshot, () => null);
  const setFailedDelivery = useCallback((failure: FailedDelivery) => {
    FAILURE_REGISTRY.set(fleetId, failure);
    notifyFailureListeners(fleetId);
  }, [fleetId]);
  const clearFailedDelivery = useCallback(() => {
    FAILURE_REGISTRY.delete(fleetId);
    notifyFailureListeners(fleetId);
  }, [fleetId]);
  return { failedDelivery, setFailedDelivery, clearFailedDelivery };
}

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetFleetDeliveryFailuresForTests(): void {
  FAILURE_REGISTRY.clear();
  for (const fleetId of FAILURE_LISTENERS.keys()) notifyFailureListeners(fleetId);
}

function notifyFailureListeners(fleetId: string): void {
  for (const listener of FAILURE_LISTENERS.get(fleetId) ?? []) listener();
}
