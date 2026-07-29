"use client";

import { useSyncExternalStore } from "react";

type SidebarListener = () => void;

export class ShellSidebarState {
  #collapsed = false;
  #listeners = new Set<SidebarListener>();

  readonly getSnapshot = (): boolean => this.#collapsed;
  readonly getServerSnapshot = (): boolean => false;

  readonly subscribe = (listener: SidebarListener): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  setCollapsed(next: boolean): void {
    if (next === this.#collapsed) return;
    this.#collapsed = next;
    for (const listener of this.#listeners) listener();
  }

  readonly toggle = (): void => {
    this.setCollapsed(!this.#collapsed);
  };

  reset(): void {
    this.setCollapsed(false);
  }
}

export const shellSidebarState = new ShellSidebarState();

export function useShellSidebarCollapsed(): boolean {
  return useSyncExternalStore(
    shellSidebarState.subscribe,
    shellSidebarState.getSnapshot,
    shellSidebarState.getServerSnapshot,
  );
}
