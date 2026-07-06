import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// The zero-workspace empty state opens the create dialog through a dynamic
// island; stub it to a marker that renders only when `open`, so the click →
// open wiring is observable without pulling next/dynamic into the test.
vi.mock("@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic", () => ({
  default: ({ open }: { open: boolean }) =>
    open ? React.createElement("div", { "data-testid": "create-dialog-open" }) : null,
}));

afterEach(() => cleanup());

describe("DashboardError boundary", () => {
  it("renders the retry surface and calls reset on click", async () => {
    const reset = vi.fn();
    const { default: DashboardError } = await import("../app/(dashboard)/error");
    render(React.createElement(DashboardError, { error: new Error("boom"), reset }));

    expect(screen.getByText(/something went wrong/i)).toBeTruthy();
    expect(screen.getByText(/couldn't load this page/i)).toBeTruthy();

    await userEvent.click(screen.getByTestId("dashboard-error-retry"));
    expect(reset).toHaveBeenCalledOnce();
  });
});

describe("NoWorkspaceEmptyState", () => {
  it("renders the create-first surface and opens the dialog on click", async () => {
    const { default: NoWorkspaceEmptyState } = await import(
      "../components/layout/NoWorkspaceEmptyState"
    );
    render(React.createElement(NoWorkspaceEmptyState));

    expect(screen.getByText(/no workspace yet/i)).toBeTruthy();
    // Closed by default.
    expect(screen.queryByTestId("create-dialog-open")).toBeNull();

    await userEvent.click(screen.getByTestId("create-first-workspace"));
    expect(screen.getByTestId("create-dialog-open")).toBeTruthy();
  });
});
