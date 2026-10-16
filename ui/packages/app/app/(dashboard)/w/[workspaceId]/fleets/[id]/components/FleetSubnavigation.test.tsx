import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

// Every tab's link status, settled unless a case says a navigation is in flight.
const linkStatus = vi.hoisted(() => ({ pending: false }));

vi.mock("next/link", async (importOriginal) => {
  const actual = await importOriginal<typeof import("next/link")>();
  return { ...actual, useLinkStatus: () => linkStatus };
});

import { FleetSubnavigation, FLEET_VIEW, resolveFleetView } from "./FleetSubnavigation";

afterEach(() => {
  linkStatus.pending = false;
  cleanup();
});

describe("FleetSubnavigation", () => {
  it("renders all fleet-local sections with one current page", () => {
    render(
      <FleetSubnavigation
        workspaceId="ws_1"
        fleetId="fleet_1"
        activeView={FLEET_VIEW.memory}
      />,
    );
    expect(screen.getAllByRole("link")).toHaveLength(5);
    expect(screen.getByRole("link", { name: "Memory" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Chat" }).getAttribute("href")).toBe("/w/ws_1/fleets/fleet_1");
    expect(screen.queryByRole("link", { name: "Settings" })).toBeNull();
  });

  it("defaults a missing view to Chat and rejects unknown views", () => {
    expect(resolveFleetView(undefined)).toBe(FLEET_VIEW.chat);
    expect(resolveFleetView(FLEET_VIEW.chat)).toBe(FLEET_VIEW.chat);
    expect(resolveFleetView("unknown")).toBeNull();
  });

  it("every fleet tab is a link that says when its view is on the way", () => {
    linkStatus.pending = true;
    render(<FleetSubnavigation workspaceId="ws_1" fleetId="fleet_1" activeView={FLEET_VIEW.chat} />);
    for (const name of ["Chat", "Events", "Memory", "Setup", "Trigger"]) {
      const label = screen.getByRole("link", { name }).querySelector("[data-pending]");
      expect(label?.getAttribute("data-pending"), name).toBe("true");
    }
  });
});
