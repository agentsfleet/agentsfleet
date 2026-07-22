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

import FleetTile, {
  FLEET_AGENT_DESCRIPTION,
  FLEET_NO_LIVE_ACTIVITY_COPY,
  FLEET_WAITING_COPY,
  MANAGE_FLEET_LABEL,
} from "./FleetTile";
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

describe("FleetTile kinds", () => {
  it("a parked fleet is a drained tile that never calls the stream hook", () => {
    const { container, getByText } = renderTile(fleet({ status: "stopped" }));
    expect(streamMock).not.toHaveBeenCalled();
    const card = container.querySelector("[data-kind]");
    expect(card?.getAttribute("data-kind")).toBe("drained");
    expect(card?.className).toContain("opacity-60");
    expect(getByText(FLEET_NO_LIVE_ACTIVITY_COPY)).toBeTruthy();
    // Every tile links to its console, drained included.
    expect(container.querySelector('a[href="/w/ws_1/fleets/flt_1"]')).not.toBeNull();
  });

  it("an active fleet renders live identity, agent purpose, management, and server truth", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByRole, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="live"]')).not.toBeNull();
    expect(container.querySelector('[data-fleet-sigil][data-live="true"]')).not.toBeNull();
    expect(getByText(FLEET_AGENT_DESCRIPTION)).toBeTruthy();
    expect(getByText(FLEET_WAITING_COPY)).toBeTruthy();
    expect(getByText(MANAGE_FLEET_LABEL, { exact: false })).toBeTruthy();
    expect(getByRole("link", { name: /manage fleet: alpha — active/i })).toBeTruthy();
    // Footer reads server truth, not token math — figures carry their unit as
    // a plain word, never an abbreviation.
    expect(getByText("$1.20 spent")).toBeTruthy();
    expect(getByText("7 events")).toBeTruthy();
    // No snapshot eyebrow while live; the pulse animates (data-live set).
    expect(container.textContent).not.toContain("snapshot");
    expect(container.querySelector('[data-live="true"]')).not.toBeNull();
  });

  it("a reconnecting stream degrades to a snapshot tile with its last event and a still sigil", async () => {
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

  it("derives a stable, distinct robot sigil from the immutable fleet id", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container } = render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(FleetTile, { fleet: fleet({ id: "fleet-alpha" }), workspaceId: "ws_1" }),
          React.createElement(FleetTile, { fleet: fleet({ id: "fleet-bravo" }), workspaceId: "ws_1" }),
          React.createElement(FleetTile, { fleet: fleet({ id: "fleet-alpha" }), workspaceId: "ws_1" }),
        ),
      ),
    );
    const sigils = Array.from(container.querySelectorAll("[data-fleet-sigil]"));
    const agentNames = Array.from(container.querySelectorAll("[data-agent-name]"));
    expect(sigils).toHaveLength(3);
    expect(agentNames).toHaveLength(3);
    expect(sigils[0]?.getAttribute("data-fleet-sigil")).toBe(
      sigils[2]?.getAttribute("data-fleet-sigil"),
    );
    expect(sigils[0]?.getAttribute("data-fleet-sigil")).not.toBe(
      sigils[1]?.getAttribute("data-fleet-sigil"),
    );
    expect(agentNames[0]?.getAttribute("data-agent-name")).toBe(
      agentNames[2]?.getAttribute("data-agent-name"),
    );
    expect(agentNames[0]?.getAttribute("data-agent-name")).not.toBe(
      agentNames[1]?.getAttribute("data-agent-name"),
    );
    expect(container.textContent).toMatch(/Agent [A-Za-z]+-[0-9A-F]{4}/);
  });

  it("preserves the canonical callsign and mirrored sigil geometry", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container } = renderTile(
      fleet({ id: "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee" }),
    );
    expect(container.querySelector('[data-fleet-sigil="4bce8453"]')).not.toBeNull();
    expect(container.querySelector('[data-agent-name="Lumen-8453"]')).not.toBeNull();

    const cells = Array.from(
      container.querySelectorAll('svg rect[fill="currentColor"]'),
      (cell) => ({
        x: Number(cell.getAttribute("x")),
        y: Number(cell.getAttribute("y")),
        width: Number(cell.getAttribute("width")),
        height: Number(cell.getAttribute("height")),
      }),
    );
    for (const cell of cells) {
      expect(cell.x).toBeGreaterThanOrEqual(4.5);
      expect(cell.x + cell.width).toBeLessThanOrEqual(19.5);
      expect(cell.y).toBeGreaterThanOrEqual(5);
      expect(cell.y + cell.height).toBeLessThanOrEqual(20);
      expect(cells).toContainEqual({ ...cell, x: 24 - cell.x - cell.width });
    }
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
    expect(container.querySelector('[data-fleet-sigil][data-live="true"]')).toBeNull();
  });

  it("an active fleet does not glow before its stream connects", () => {
    streamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      helloReceived: false,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-fleet-sigil][data-live="true"]')).toBeNull();
    expect(getByText(FLEET_NO_LIVE_ACTIVITY_COPY)).toBeTruthy();
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
