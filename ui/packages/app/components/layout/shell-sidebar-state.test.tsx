import { describe, expect, it, vi } from "vitest";

import { ShellSidebarState } from "./shell-sidebar-state";

describe("shell sidebar state", () => {
  it("notifies every active island with the same collapse snapshot", () => {
    const state = new ShellSidebarState();
    const observed: boolean[] = [];
    const unsubscribe = state.subscribe(() => observed.push(state.getSnapshot()));

    expect(state.getServerSnapshot()).toBe(true);
    expect(state.getSnapshot()).toBe(true);
    state.toggle();
    state.toggle();
    unsubscribe();
    state.toggle();

    expect(observed).toEqual([false, true]);
    expect(state.getSnapshot()).toBe(false);
  });

  it("resets once and ignores idempotent writes", () => {
    const state = new ShellSidebarState();
    const listener = vi.fn();
    state.subscribe(listener);

    state.reset();
    state.setCollapsed(true);
    expect(listener).not.toHaveBeenCalled();

    state.setCollapsed(false);
    state.setCollapsed(false);
    state.reset();
    state.reset();
    expect(listener).toHaveBeenCalledTimes(2);
    expect(state.getSnapshot()).toBe(true);
  });
});
