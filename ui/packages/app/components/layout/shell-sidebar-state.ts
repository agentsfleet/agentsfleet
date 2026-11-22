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

// Literal classes keep both widths visible to Tailwind's source scanner.
export const SIDEBAR_COLUMN = {
  aside: { expanded: "w-14 summary:w-18 md:w-55", collapsed: "w-14 summary:w-18" },
} as const;

export const shellSidebarState = new ShellSidebarState();

export function useShellSidebarCollapsed(): boolean {
  return useSyncExternalStore(
    shellSidebarState.subscribe,
    shellSidebarState.getSnapshot,
    shellSidebarState.getServerSnapshot,
  );
}

// Only the visible navigation owns private history requests. On a phone the
// dialog mounts its own history when opened.
const COMPACT_NAV = "(width < 48rem)";
function subscribeViewport(changed: () => void) {
  const query = window.matchMedia(COMPACT_NAV);
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}
const desktopViewport = () => !window.matchMedia(COMPACT_NAV).matches;
const viewportUnknown = () => false;
export function useDesktopNavigationViewport(): boolean {
  return useSyncExternalStore(subscribeViewport, desktopViewport, viewportUnknown);
}
