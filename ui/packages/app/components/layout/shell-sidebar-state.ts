"use client";

import { useSyncExternalStore } from "react";

type SidebarListener = () => void;

export class ShellSidebarState {
  #collapsed = true;
  #listeners = new Set<SidebarListener>();

  readonly getSnapshot = (): boolean => this.#collapsed;
  readonly getServerSnapshot = (): boolean => true;

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
    this.setCollapsed(true);
  }
}

// Literal classes keep both widths visible to Tailwind's source scanner.
export const SIDEBAR_COLUMN = {
  aside: { expanded: "w-14 md:w-55", collapsed: "w-14 md:w-18" },
} as const;

export const shellSidebarState = new ShellSidebarState();

export function useShellSidebarCollapsed(): boolean {
  return useSyncExternalStore(
    shellSidebarState.subscribe,
    shellSidebarState.getSnapshot,
    shellSidebarState.getServerSnapshot,
  );
}
