import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { resetCommonMocks } from "./helpers/dashboard-mocks";

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@/app/(dashboard)/fleets/actions", () => ({
  onboardLibraryEntryAction: vi.fn(),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

import AddLibraryDialog from "../app/(dashboard)/fleets/new/AddLibraryDialog";
import { InstallSourceSelector } from "../app/(dashboard)/fleets/new/InstallSourceSelector";

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
});
afterEach(() => cleanup());

// Regression: the dashboard empty-state CTA deep-links /fleets/new?create=1 so
// the create-template form opens on arrival instead of re-rendering the same
// empty state behind a second identical button.
describe("AddLibraryDialog deep link", () => {
  it("opens on first render when defaultOpen is set", async () => {
    render(React.createElement(AddLibraryDialog, { workspaceId: "ws_1", defaultOpen: true }));
    expect(await screen.findByLabelText("Repository")).toBeTruthy();
  });

  it("stays closed on first render without defaultOpen", () => {
    render(React.createElement(AddLibraryDialog, { workspaceId: "ws_1" }));
    expect(screen.queryByLabelText("Repository")).toBeNull();
  });
});

describe("InstallSourceSelector deep link", () => {
  it("opens the create dialog from the empty state when initialCreateOpen is set", async () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        entries: [],
        onUseLibraryEntry: () => {},
        canAddLibraryEntry: true,
        initialCreateOpen: true,
      }),
    );
    expect(await screen.findByLabelText("Repository")).toBeTruthy();
  });
});
