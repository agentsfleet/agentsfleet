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

/*
 * The sidebar column's width in each state, spelled once for both readers.
 *
 * The `<aside>` renders the column; the shell header renders a leading cluster
 * over it, and the collapse toggle sits at that cluster's trailing edge. The
 * toggle therefore lines up with the column it collapses only while the two
 * widths agree — so they are read from here rather than typed twice, and
 * `test_the_header_cluster_is_the_sidebar_column` fails when they drift.
 *
 * Both spellings are literal because Tailwind scans source text: a computed
 * `md:${...}` would never reach the generated stylesheet.
 */
export const SIDEBAR_COLUMN = {
  /** The column itself, inside an `<aside>` that is already `md:`-gated. */
  aside: { expanded: "w-60", collapsed: "w-16" },
  /** The header's leading cluster, which is a column only from `md` up. */
  header: { expanded: "md:w-60", collapsed: "md:w-16" },
} as const;

export const shellSidebarState = new ShellSidebarState();

export function useShellSidebarCollapsed(): boolean {
  return useSyncExternalStore(
    shellSidebarState.subscribe,
    shellSidebarState.getSnapshot,
    shellSidebarState.getServerSnapshot,
  );
}
