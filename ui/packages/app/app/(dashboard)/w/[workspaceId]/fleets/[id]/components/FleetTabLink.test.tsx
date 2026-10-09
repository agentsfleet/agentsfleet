import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

const linkStatus = vi.hoisted(() => ({ pending: false }));

vi.mock("next/link", async (importOriginal) => {
  const actual = await importOriginal<typeof import("next/link")>();
  return { ...actual, useLinkStatus: () => linkStatus };
});

import { FleetTabLink } from "./FleetTabLink";

afterEach(() => {
  linkStatus.pending = false;
  cleanup();
});

describe("FleetTabLink", () => {
  it("a clicked fleet tab reads as loading while its view is on the way", () => {
    linkStatus.pending = true;
    render(<FleetTabLink href="/w/ws_1/fleets/fleet_1?view=events">Events</FleetTabLink>);
    const label = screen.getByText("Events");
    expect(label.getAttribute("data-pending")).toBe("true");
    expect(label.className).toContain("animate-pulse");
    // The link brightens off its pending label; the underline stays put.
    expect(screen.getByRole("link", { name: "Events" }).className).toContain("has-data-[pending=true]:text-foreground");
  });

  it("a settled tab carries no pending marker", () => {
    render(<FleetTabLink href="/w/ws_1/fleets/fleet_1?view=memory">Memory</FleetTabLink>);
    const label = screen.getByText("Memory");
    expect(label.getAttribute("data-pending")).toBeNull();
    expect(label.className).not.toContain("animate-pulse");
  });
});
