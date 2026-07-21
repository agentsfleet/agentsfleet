"use client";

import { useCallback, useEffect, useSyncExternalStore } from "react";
import {
  createMessageQueue,
  type AppendMessage,
  type ExternalThreadQueueAdapter,
  type MessageQueueController,
} from "@assistant-ui/react";

export const QUEUE_DELIVERY = {
  WAITING: "waiting",
  COMPLETE: "complete",
  FAILED: "failed",
} as const;

export type QueueDeliveryResult =
  (typeof QUEUE_DELIVERY)[keyof typeof QUEUE_DELIVERY];

type DeliverMessage = (message: AppendMessage) => Promise<QueueDeliveryResult>;

type FleetQueueEntry = {
  controller: MessageQueueController;
  setDeliver: (deliverMessage: DeliverMessage) => void;
  retain: () => void;
  release: () => void;
  hasConsumers: () => boolean;
  cleanupTimer: ReturnType<typeof setTimeout> | null;
};

const EMPTY_QUEUE_ITEMS: ExternalThreadQueueAdapter["items"] = [];
const QUEUE_RELEASE_MS = 30_000;
const QUEUE_REGISTRY = new Map<string, FleetQueueEntry>();
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

export function useFleetMessageQueue(
  fleetId: string,
  isBusy: boolean,
  deliverMessage: DeliverMessage,
): {
  queue: ExternalThreadQueueAdapter;
  retryMessage: (message: AppendMessage) => void;
} {
  const entry = queueEntryFor(fleetId, deliverMessage);
  const { controller } = entry;
  entry.setDeliver(deliverMessage);

  useSyncExternalStore(
    controller.subscribe,
    () => controller.adapter.items,
    () => EMPTY_QUEUE_ITEMS,
  );

  useEffect(() => {
    retainQueueEntry(entry);
    return () => releaseQueueEntry(fleetId, entry);
  }, [entry, fleetId]);

  useEffect(() => {
    if (isBusy) {
      controller.notifyBusy();
    } else {
      controller.notifyIdle();
    }
  }, [controller, isBusy]);

  const retryMessage = useCallback((message: AppendMessage) => {
    controller.adapter.enqueue(message, { steer: true });
    if (!isBusy) controller.notifyIdle();
  }, [controller, isBusy]);

  return { queue: controller.adapter, retryMessage };
}

function queueEntryFor(
  fleetId: string,
  deliverMessage: DeliverMessage,
): FleetQueueEntry {
  if (typeof window === "undefined") return createQueueEntry(deliverMessage);
  const existing = QUEUE_REGISTRY.get(fleetId);
  if (existing) return existing;
  const entry = createQueueEntry(deliverMessage);
  QUEUE_REGISTRY.set(fleetId, entry);
  return entry;
}

function createQueueEntry(deliverMessage: DeliverMessage): FleetQueueEntry {
  let deliver = deliverMessage;
  let consumers = 0;
  const controller: MessageQueueController = createMessageQueue({
    run: (message) => {
      void deliver(message)
        .then((result) => {
          if (result === QUEUE_DELIVERY.COMPLETE && consumers > 0) {
            controller.notifyIdle();
          }
        })
        .catch(() => undefined);
    },
  });
  return {
    controller,
    setDeliver: (next) => {
      deliver = next;
    },
    retain: () => {
      consumers += 1;
    },
    release: () => {
      consumers = Math.max(0, consumers - 1);
    },
    hasConsumers: () => consumers > 0,
    cleanupTimer: null,
  };
}

function retainQueueEntry(entry: FleetQueueEntry): void {
  if (entry.cleanupTimer) clearTimeout(entry.cleanupTimer);
  entry.cleanupTimer = null;
  entry.retain();
}

function releaseQueueEntry(fleetId: string, entry: FleetQueueEntry): void {
  entry.release();
  if (entry.hasConsumers()) return;
  entry.controller.notifyBusy();
  if (entry.controller.adapter.items.length > 0) return;
  entry.cleanupTimer = setTimeout(() => {
    QUEUE_REGISTRY.delete(fleetId);
  }, QUEUE_RELEASE_MS);
}

export function __resetFleetMessageQueuesForTests(): void {
  for (const entry of QUEUE_REGISTRY.values()) {
    if (entry.cleanupTimer) clearTimeout(entry.cleanupTimer);
    entry.controller.adapter.clear("cancel-run");
  }
  QUEUE_REGISTRY.clear();
  FAILURE_REGISTRY.clear();
  for (const fleetId of FAILURE_LISTENERS.keys()) notifyFailureListeners(fleetId);
}

function notifyFailureListeners(fleetId: string): void {
  for (const listener of FAILURE_LISTENERS.get(fleetId) ?? []) listener();
}
