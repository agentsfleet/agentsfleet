import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import type { Fleet } from "@/lib/api/fleets";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href, ...rest }, children),
}));

// The streaming hook is the tile's only live dependency; a stub lets each test
// pin connectionStatus + the last event without a real EventSource.
const streamMock = vi.fn();
vi.mock("@/components/domain/useWorkspaceStream", () => ({
  useWorkspaceFleetStream: (...a: unknown[]) => streamMock(...a),
}));

import FleetTile from "./FleetTile";
import {
  TILE_CATCHING_UP_EYEBROW,
  TILE_EVENTS_SUFFIX,
  TILE_NOT_LIVE_EYEBROW,
  TILE_NOT_LIVE_TOOLTIP,
  TILE_SPEND_SUFFIX,
} from "@/lib/wall/tile-liveness";

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
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(FleetTile, { fleet: f, workspaceId: "ws_1" }),
    ),
  );
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
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="live"]')).not.toBeNull();
    // Footer reads server truth, not token math — figures carry their unit as
    // a plain word, never an abbreviation.
    expect(getByText("$1.20 spent")).toBeTruthy();
    expect(getByText("7 events")).toBeTruthy();
    // No snapshot eyebrow while live; the pulse animates (data-live set).
    expect(container.textContent).not.toContain("snapshot");
    expect(container.querySelector('[data-live="true"]')).not.toBeNull();
  });

  it("a reconnecting stream degrades to a snapshot tile with its last event, pulse STILL (2.2, 2.3)", async () => {
    streamMock.mockReturnValue({
      events: [{ id: "e1", role: "assistant", actor: "fleet", text: "ran a check", createdAt: new Date(0), status: "received" }],
      connectionStatus: CONNECTION_STATUS.RECONNECTING,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByText, getAllByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="snapshot"]')).not.toBeNull();
    const notLive = getByText(TILE_NOT_LIVE_EYEBROW);
    expect(notLive).toBeTruthy();
    // The plain-words eyebrow carries the one-sentence explanation in a real
    // tooltip, reachable by keyboard focus (a bare title attribute would be
    // unreachable under the tile's pointer-events-none wrapper).
    fireEvent.focus(notLive);
    await waitFor(() => expect(getAllByText(TILE_NOT_LIVE_TOOLTIP).length).toBeGreaterThan(0));
    // Mouse reachability is two separate escapes and losing either one silently
    // kills the tooltip: the trigger must opt back into pointer events AND
    // stack above the card-wide absolute link, which otherwise paints over all
    // in-flow content and swallows the hover.
    expect(notLive.className).toContain("pointer-events-auto");
    expect(notLive.className).toContain("relative");
    expect(notLive.className).toContain("z-10");
    expect(getByText("ran a check")).toBeTruthy();
    // The pulse must NOT animate in snapshot mode — the animation is live-only,
    // so a frozen feed cannot masquerade as live (greptile P2, DESIGN_SYSTEM §Motion).
    expect(container.querySelector('[data-live="true"]')).toBeNull();
  });

  it("an installing fleet streams with an info-toned marker", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      helloReceived: false,
      isLive: true,
      catchingUp: false,
    });
    const { container } = renderTile(fleet({ status: "installing" }));
    expect(container.querySelector(".bg-info")).not.toBeNull();
  });

  it("a fleet the daemon sent no aggregates for renders dashes, not $0.00", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { getByText } = renderTile(fleet({ budget_used_nanos: undefined, events_processed: undefined }));
    expect(getByText("— spent")).toBeTruthy();
    expect(getByText("— events")).toBeTruthy();
  });

  it("a tile absent from the server hello set renders snapshot, not live", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: false,
      catchingUp: false,
    });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="snapshot"]')).not.toBeNull();
    expect(getByText(TILE_NOT_LIVE_EYEBROW)).toBeTruthy();
    expect(container.querySelector('[data-live="true"]')).toBeNull();
  });

  it("test_wall_copy_consts_are_single_source", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.RECONNECTING,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByText } = renderTile(fleet());
    // Every rendered label is the exported constant — the component and this
    // test read the same source, so a copy change lands in exactly one place.
    expect(getByText(TILE_NOT_LIVE_EYEBROW)).toBeTruthy();
    expect(container.textContent).toContain(`7 ${TILE_EVENTS_SUFFIX}`);
    expect(container.textContent).toContain(`$1.20 ${TILE_SPEND_SUFFIX}`);
  });

  it("a server drop signal surfaces catching up", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: true,
    });
    const { getByText } = renderTile(fleet());
    expect(getByText(TILE_CATCHING_UP_EYEBROW)).toBeTruthy();
  });
});
