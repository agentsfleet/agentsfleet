import { describe, expect, it, vi } from "vitest";

import { ShellSidebarState } from "./shell-sidebar-state";

describe("shell sidebar state", () => {
  it("notifies every active island with the same collapse snapshot", () => {
    const state = new ShellSidebarState();
    const observed: boolean[] = [];
    const unsubscribe = state.subscribe(() => observed.push(state.getSnapshot()));

    expect(state.getServerSnapshot()).toBe(false);
    expect(state.getSnapshot()).toBe(false);
    state.toggle();
    state.toggle();
    unsubscribe();
    state.toggle();

    expect(observed).toEqual([true, false]);
    expect(state.getSnapshot()).toBe(true);
  });

  it("resets once and ignores idempotent writes", () => {
    const state = new ShellSidebarState();
    const listener = vi.fn();
    state.subscribe(listener);

    state.reset();
    state.setCollapsed(false);
    expect(listener).not.toHaveBeenCalled();

    state.setCollapsed(true);
    state.setCollapsed(true);
    state.reset();
    state.reset();
    expect(listener).toHaveBeenCalledTimes(2);
    expect(state.getSnapshot()).toBe(false);
  });
});
