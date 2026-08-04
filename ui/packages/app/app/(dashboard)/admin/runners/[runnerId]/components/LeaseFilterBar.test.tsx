import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { LeaseFilterBar, type LeaseFilterState } from "./LeaseFilterBar";
import {
  APPLY_LEASE_FILTER_LABEL,
  CLEAR_FLEET_FILTER_LABEL,
  CLEAR_LEASE_FILTER_LABEL,
  CLEAR_WORKSPACE_FILTER_LABEL,
  LEASE_FILTER_LABEL,
} from "./runner-copy";

// `LeaseFilterBar` takes its state as a prop rather than calling
// `useLeaseFilters` itself, so these render against a stub and never touch the
// router. The hook's own URL-writing half is covered through `LeaseTable`, which
// mounts the real one.
//
// The keyboard path is the reason this file exists. An operator types a query
// and presses Enter; nothing else in the suite presses that key, so the handler
// could be deleted and every other lease-filter test would still pass.

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0011";
const FLEET_NAME = "billing-reconciler";

afterEach(() => cleanup());

function filterState(overrides: Partial<LeaseFilterState> = {}): LeaseFilterState {
  return {
    workspace: null,
    fleet: null,
    apply: vi.fn(),
    clearWorkspace: vi.fn(),
    clearFleet: vi.fn(),
    clearAll: vi.fn(),
    ...overrides,
  };
}

function renderBar(filters: LeaseFilterState) {
  render(
    <TooltipProvider>
      <LeaseFilterBar filters={filters} />
    </TooltipProvider>,
  );
  return screen.getByLabelText(LEASE_FILTER_LABEL);
}

describe("LeaseFilterBar keyboard submit", () => {
  it("applies the typed query when Enter is pressed, without reaching for the button", () => {
    const filters = filterState();
    const input = renderBar(filters);

    fireEvent.change(input, { target: { value: `fleet:${FLEET_NAME}` } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(filters.apply).toHaveBeenCalledTimes(1);
    expect(filters.apply).toHaveBeenCalledWith({ workspace: null, fleet: FLEET_NAME });
  });

  it("submits both tokens together, so Enter never drops the half the operator did not retype", () => {
    const filters = filterState();
    const input = renderBar(filters);

    fireEvent.change(input, {
      target: { value: `workspace:${WORKSPACE_ID} fleet:${FLEET_NAME}` },
    });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(filters.apply).toHaveBeenCalledWith({ workspace: WORKSPACE_ID, fleet: FLEET_NAME });
  });

  it("does not navigate on any other key — typing is not submitting", () => {
    // The guard's false arm. Without this, `if (event.key === "Enter")` could be
    // dropped entirely and the Enter test above would still pass, because every
    // keystroke would submit.
    const filters = filterState();
    const input = renderBar(filters);

    fireEvent.change(input, { target: { value: `fleet:${FLEET_NAME}` } });
    fireEvent.keyDown(input, { key: "a" });
    fireEvent.keyDown(input, { key: "Escape" });
    fireEvent.keyDown(input, { key: "Tab" });

    expect(filters.apply).not.toHaveBeenCalled();
  });

  it("applies what is in the box now, not what was there when the row first rendered", () => {
    // Enter reads the live draft. A handler closing over a stale value would
    // submit the previous query and silently strand the operator a filter behind.
    const filters = filterState();
    const input = renderBar(filters);

    fireEvent.change(input, { target: { value: "fleet:first-guess" } });
    fireEvent.change(input, { target: { value: `fleet:${FLEET_NAME}` } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(filters.apply).toHaveBeenCalledTimes(1);
    expect(filters.apply).toHaveBeenCalledWith({ workspace: null, fleet: FLEET_NAME });
  });

  it("clears both filters through Enter on an emptied box, rather than leaving the old pair applied", () => {
    // Clearing the text and pressing Enter parses to two nulls — the same shape
    // the Clear-all button produces, reached by keyboard.
    const filters = filterState({ workspace: WORKSPACE_ID, fleet: FLEET_NAME });
    const input = renderBar(filters);

    fireEvent.change(input, { target: { value: "" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(filters.apply).toHaveBeenCalledWith({ workspace: null, fleet: null });
  });

  it("reaches the same handler through the Apply button, so the two paths cannot diverge", () => {
    const filters = filterState();
    const input = renderBar(filters);

    fireEvent.change(input, { target: { value: `fleet:${FLEET_NAME}` } });
    fireEvent.click(screen.getByRole("button", { name: APPLY_LEASE_FILTER_LABEL }));

    expect(filters.apply).toHaveBeenCalledWith({ workspace: null, fleet: FLEET_NAME });
  });
});

describe("LeaseFilterBar active-filter chips", () => {
  it("shows no chips and no clear-all while the feed is unfiltered", () => {
    const filters = filterState();
    renderBar(filters);

    expect(screen.queryByRole("button", { name: CLEAR_LEASE_FILTER_LABEL })).toBeNull();
    expect(screen.queryByRole("button", { name: CLEAR_WORKSPACE_FILTER_LABEL })).toBeNull();
    expect(screen.queryByRole("button", { name: CLEAR_FLEET_FILTER_LABEL })).toBeNull();
  });

  it("drops the workspace alone, so clearing one chip does not discard the other filter", () => {
    // The whole reason the chips exist. A single clear-all would force the
    // operator to retype the filter they wanted to keep.
    const filters = filterState({ workspace: WORKSPACE_ID, fleet: FLEET_NAME });
    renderBar(filters);

    fireEvent.click(screen.getByRole("button", { name: CLEAR_WORKSPACE_FILTER_LABEL }));

    expect(filters.clearWorkspace).toHaveBeenCalledTimes(1);
    expect(filters.clearFleet).not.toHaveBeenCalled();
    expect(filters.clearAll).not.toHaveBeenCalled();
  });

  it("drops the fleet alone, leaving the workspace filter applied", () => {
    const filters = filterState({ workspace: WORKSPACE_ID, fleet: FLEET_NAME });
    renderBar(filters);

    fireEvent.click(screen.getByRole("button", { name: CLEAR_FLEET_FILTER_LABEL }));

    expect(filters.clearFleet).toHaveBeenCalledTimes(1);
    expect(filters.clearWorkspace).not.toHaveBeenCalled();
    expect(filters.clearAll).not.toHaveBeenCalled();
  });

  it("offers only the fleet chip when the workspace is unfiltered", () => {
    // Each chip is gated on its own filter. A chip rendered for an absent filter
    // would offer to clear something that is not applied.
    const filters = filterState({ fleet: FLEET_NAME });
    renderBar(filters);

    expect(screen.queryByRole("button", { name: CLEAR_WORKSPACE_FILTER_LABEL })).toBeNull();
    expect(screen.getByRole("button", { name: CLEAR_FLEET_FILTER_LABEL })).toBeTruthy();
  });

  it("offers only the workspace chip when the fleet is unfiltered, showing the id shortened", () => {
    const filters = filterState({ workspace: WORKSPACE_ID });
    renderBar(filters);

    expect(screen.queryByRole("button", { name: CLEAR_FLEET_FILTER_LABEL })).toBeNull();
    expect(screen.getByRole("button", { name: CLEAR_WORKSPACE_FILTER_LABEL })).toBeTruthy();
    // Shortened in the chip, full value on the title — an operator can still
    // read the whole id on hover without the chip eating the toolbar.
    expect(screen.getByTitle(WORKSPACE_ID)).toBeTruthy();
  });

  it("clears both filters at once through the clear-all button", () => {
    const filters = filterState({ workspace: WORKSPACE_ID, fleet: FLEET_NAME });
    renderBar(filters);

    fireEvent.click(screen.getByRole("button", { name: CLEAR_LEASE_FILTER_LABEL }));

    expect(filters.clearAll).toHaveBeenCalledTimes(1);
    expect(filters.clearWorkspace).not.toHaveBeenCalled();
    expect(filters.clearFleet).not.toHaveBeenCalled();
  });
});
