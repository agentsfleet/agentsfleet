import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FleetSubnavigation, FLEET_VIEW, resolveFleetView } from "./FleetSubnavigation";

afterEach(() => cleanup());

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
    expect(screen.getByRole("link", { name: "Memory" }).querySelector("svg")?.getAttribute("class"))
      .toContain("lucide-brain");
    // The app's one tab style — an underline over a hairline rail, shared
    // with Billing — and one strip at every width, no `lg:` rail variant.
    expect(screen.getByRole("navigation").className).toContain("border-b");
    expect(screen.getByRole("navigation").className).not.toMatch(/\blg:/);
    expect(screen.getByRole("link", { name: "Memory" }).className).toContain("border-b-2");
    expect(screen.getByRole("link", { name: "Memory" }).className).not.toContain("rounded-md");
  });

  it("defaults a missing view to Chat and rejects unknown views", () => {
    expect(resolveFleetView(undefined)).toBe(FLEET_VIEW.chat);
    expect(resolveFleetView(FLEET_VIEW.chat)).toBe(FLEET_VIEW.chat);
    expect(resolveFleetView("unknown")).toBeNull();
  });
});
