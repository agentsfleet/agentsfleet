import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { CreatedWorkspace } from "./useWorkspaceCreation";

const { pathnameMock, createdWorkspacesMock } = vi.hoisted(() => ({
  pathnameMock: vi.fn<() => string>(() => "/"),
  createdWorkspacesMock: vi.fn<() => readonly CreatedWorkspace[]>(() => []),
}));

vi.mock("next/navigation", () => ({ usePathname: () => pathnameMock() }));
vi.mock("./WorkspaceCreationProvider", () => ({
  useCreatedWorkspaces: () => createdWorkspacesMock(),
}));

import WorkspaceSwitcher from "./WorkspaceSwitcher";

const LISTED = [
  { id: "0195b4ba-8d3a-7f13-8abc-b00000000001", name: "listed-alpha", created_at: 0 },
  { id: "0195b4ba-8d3a-7f13-8abc-b00000000002", name: "listed-beta", created_at: 0 },
];
const CREATED_ID = "0195b4ba-8d3a-7f13-8abc-b00000000009";
const UNKNOWN_ID = "0195b4ba-8d3a-7f13-8abc-b00000000042";

function renderAt(workspaceId: string) {
  pathnameMock.mockReturnValue(`/w/${workspaceId}/fleets`);
  return render(
    React.createElement(WorkspaceSwitcher, { workspaces: LISTED }),
  );
}

beforeEach(() => {
  pathnameMock.mockReset();
  pathnameMock.mockReturnValue("/");
  createdWorkspacesMock.mockReset();
  createdWorkspacesMock.mockReturnValue([]);
});
afterEach(() => cleanup());

// The trigger is what survives the navigation that workspace creation fires
// (the lazy menu unmounts, the shared layout's list is stale), so IT must
// resolve a just-created workspace's name — the placeholder here was the
// user-visible regression.
describe("WorkspaceSwitcher label resolution", () => {
  it("resolves a routed workspace found only in the creation context", () => {
    createdWorkspacesMock.mockReturnValue([{ id: CREATED_ID, name: "fresh-workspace" }]);
    renderAt(CREATED_ID);
    expect(screen.getByTestId("workspace-switcher").textContent).toContain("fresh-workspace");
  });

  it("resolves a routed workspace from the server-provided list", () => {
    renderAt(LISTED[1]!.id);
    expect(screen.getByTestId("workspace-switcher").textContent).toContain("listed-beta");
  });

  it("keeps the placeholder for a routed id known to neither source", () => {
    createdWorkspacesMock.mockReturnValue([{ id: CREATED_ID, name: "fresh-workspace" }]);
    renderAt(UNKNOWN_ID);
    expect(screen.getByTestId("workspace-switcher").textContent).toContain("Current workspace");
  });

  it("prefers the server list over the creation context for the same id", () => {
    // Once the layout catches up, the server row is the durable truth (the
    // optimistic row may carry a null name from a race with naming).
    createdWorkspacesMock.mockReturnValue([{ id: LISTED[0]!.id, name: null }]);
    renderAt(LISTED[0]!.id);
    expect(screen.getByTestId("workspace-switcher").textContent).toContain("listed-alpha");
  });
});
