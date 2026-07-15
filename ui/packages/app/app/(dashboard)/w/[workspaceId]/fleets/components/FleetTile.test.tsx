import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import type { Fleet } from "@/lib/api/fleets";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href, ...rest }, children),
}));

// The streaming hook is the tile's only live dependency; a stub lets each test
// pin connectionStatus + the last event without a real EventSource.
const streamMock = vi.fn();
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: (...a: unknown[]) => streamMock(...a),
}));

import FleetTile from "./FleetTile";

function fleet(over: Partial<Fleet> = {}): Fleet {
  return {
    id: "flt_1",
    name: "alpha",
    status: "active",
    created_at: 0,
    updated_at: 0,
    budget_used_nanos: 1_200_000_000,
    events_processed: 7,
    ...over,
  };
}

function renderTile(f: Fleet) {
  return render(React.createElement(FleetTile, { fleet: f, workspaceId: "ws_1" }));
}

afterEach(() => {
  cleanup();
  streamMock.mockReset();
});

describe("FleetTile — the three kinds (Inv. 1)", () => {
  it("a parked fleet is a drained tile that never calls the stream hook (1.3)", () => {
    const { container } = renderTile(fleet({ status: "stopped" }));
    expect(streamMock).not.toHaveBeenCalled();
    const card = container.querySelector("[data-kind]");
    expect(card?.getAttribute("data-kind")).toBe("drained");
    expect(card?.className).toContain("opacity-60");
    // Every tile links to its console, drained included.
    expect(container.querySelector('a[href="/w/ws_1/fleets/flt_1"]')).not.toBeNull();
  });

  it("an active fleet with a live stream renders the live kind + server-truth footer, pulse animating (1.2, 2.1)", () => {
    streamMock.mockReturnValue({ events: [], connectionStatus: CONNECTION_STATUS.LIVE });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="live"]')).not.toBeNull();
    // Footer reads server truth, not token math.
    expect(getByText("$1.20")).toBeTruthy();
    expect(getByText("7 ev")).toBeTruthy();
    // No snapshot eyebrow while live; the pulse animates (data-live set).
    expect(container.textContent).not.toContain("snapshot");
    expect(container.querySelector('[data-live="true"]')).not.toBeNull();
  });

  it("a reconnecting stream degrades to a snapshot tile with its last event, pulse STILL (2.2, 2.3)", () => {
    streamMock.mockReturnValue({
      events: [{ id: "e1", role: "assistant", actor: "fleet", text: "ran a check", createdAt: new Date(0), status: "received" }],
      connectionStatus: CONNECTION_STATUS.RECONNECTING,
    });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="snapshot"]')).not.toBeNull();
    expect(getByText("snapshot")).toBeTruthy();
    expect(getByText("ran a check")).toBeTruthy();
    // The pulse must NOT animate in snapshot mode — the animation is live-only,
    // so a frozen feed cannot masquerade as live (greptile P2, DESIGN_SYSTEM §Motion).
    expect(container.querySelector('[data-live="true"]')).toBeNull();
  });

  it("an installing fleet streams with an info-toned marker", () => {
    streamMock.mockReturnValue({ events: [], connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderTile(fleet({ status: "installing" }));
    expect(container.querySelector(".bg-info")).not.toBeNull();
  });

  it("a fleet the daemon sent no aggregates for renders dashes, not $0.00", () => {
    streamMock.mockReturnValue({ events: [], connectionStatus: CONNECTION_STATUS.LIVE });
    const { getByText } = renderTile(fleet({ budget_used_nanos: undefined, events_processed: undefined }));
    expect(getByText("—")).toBeTruthy();
    expect(getByText("— ev")).toBeTruthy();
  });
});
