"use client";

import { useSyncExternalStore } from "react";

export const INTENT_MODULE_STATUS = {
  idle: "idle",
  loading: "loading",
  ready: "ready",
  error: "error",
} as const;

type IntentModuleStatus =
  (typeof INTENT_MODULE_STATUS)[keyof typeof INTENT_MODULE_STATUS];

export type IntentModuleSnapshot<T> = {
  error: unknown;
  module: T | null;
  status: IntentModuleStatus;
};

export type IntentModuleLoader<T> = {
  getSnapshot: () => IntentModuleSnapshot<T>;
  preload: () => Promise<T>;
  retry: () => Promise<T>;
  subscribe: (listener: () => void) => () => void;
};

export function createIntentModuleLoader<T>(
  importModule: () => Promise<T>,
): IntentModuleLoader<T> {
  let snapshot: IntentModuleSnapshot<T> = {
    error: null,
    module: null,
    status: INTENT_MODULE_STATUS.idle,
  };
  let inFlight: Promise<T> | null = null;
  const listeners = new Set<() => void>();

  function publish(next: IntentModuleSnapshot<T>) {
    snapshot = next;
    for (const listener of listeners) listener();
  }

  function preload(): Promise<T> {
    if (snapshot.status === INTENT_MODULE_STATUS.ready) {
      return Promise.resolve(snapshot.module as T);
    }
    if (inFlight) return inFlight;

    publish({
      error: null,
      module: null,
      status: INTENT_MODULE_STATUS.loading,
    });

    let imported: Promise<T>;
    try {
      imported = importModule();
    } catch (error) {
      imported = Promise.reject(error);
    }

    const request = imported
      .then((module) => {
        publish({
          error: null,
          module,
          status: INTENT_MODULE_STATUS.ready,
        });
        return module;
      })
      .catch((error: unknown) => {
        publish({
          error,
          module: null,
          status: INTENT_MODULE_STATUS.error,
        });
        throw error;
      })
      .finally(() => {
        if (inFlight === request) inFlight = null;
      });

    inFlight = request;
    void request.catch(() => {});
    return request;
  }

  return {
    getSnapshot: () => snapshot,
    preload,
    retry() {
      if (snapshot.status !== INTENT_MODULE_STATUS.error) return preload();
      inFlight = null;
      return preload();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export function useIntentModule<T>(
  loader: IntentModuleLoader<T>,
): IntentModuleSnapshot<T> {
  return useSyncExternalStore(
    loader.subscribe,
    loader.getSnapshot,
    loader.getSnapshot,
  );
}

export function maySpeculateOnHover(): boolean {
  if (typeof window === "undefined") return false;
  if (
    typeof window.matchMedia === "function" &&
    window.matchMedia("(pointer: coarse)").matches
  ) {
    return false;
  }
  if (typeof navigator === "undefined") return true;
  const connection = (navigator as { connection?: { saveData?: boolean } })
    .connection;
  return connection?.saveData !== true;
}
