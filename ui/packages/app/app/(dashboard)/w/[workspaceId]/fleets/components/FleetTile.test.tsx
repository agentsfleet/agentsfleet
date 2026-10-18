import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import type { Fleet } from "@/lib/api/fleets";
import { agentDisplayName } from "@/lib/fleets/agent-label";

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
  FLEET_NO_LIVE_ACTIVITY_COPY,
  FLEET_WAITING_COPY,
  MANAGE_FLEET_LABEL,
} from "./FleetTile";
import {
  TILE_CATCHING_UP_EYEBROW,
  TILE_NOT_LIVE_EYEBROW,
  TILE_NOT_LIVE_TOOLTIP,
} from "@/lib/wall/tile-liveness";

/** The fade a drained tile once wore; it reads at full strength now. */
const FADED_CLASS = "opacity-60";

function fleet(over: Partial<Fleet> = {}): Fleet {
  return {
    id: "flt_1",
    name: "alpha",
    status: "active",
    created_at: "1970-01-01T00:00:00Z",
    updated_at: "1970-01-01T00:00:00Z",
    budget_used_nanos: 1_200_000_000n,
    events_processed: 7n,
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
    expect(card?.className).not.toContain(FADED_CLASS);
    expect(getByText(FLEET_NO_LIVE_ACTIVITY_COPY)).toBeTruthy();
    expect(container.querySelector("[data-fleet-status]")?.textContent).toBe("Stopped");
    // Every tile links to its console, drained included.
    expect(container.querySelector('a[href="/w/ws_1/fleets/flt_1"]')).not.toBeNull();
  });

  it("an active fleet renders live identity, its activity, management, and server truth", () => {
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByRole, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-kind="live"]')).not.toBeNull();
    expect(container.querySelector('[data-fleet-sigil][data-live="true"]')).not.toBeNull();
    expect(getByText(FLEET_WAITING_COPY)).toBeTruthy();
    expect(getByText("Open →")).toBeTruthy();
    expect(
      getByRole("link", { name: /manage fleet: alpha — agent [a-z]+-[0-9a-f]{4} — active/i }),
    ).toBeTruthy();
    // Footer reads server truth, not token math — figures carry their unit as
    // a plain word, never an abbreviation.
    expect(getByText("spent").textContent).toBe("$1.20 spent");
    expect(getByText("events").textContent).toBe("7 events");
    // No snapshot eyebrow while live; the pulse animates (data-live set).
    expect(container.textContent).not.toContain("snapshot");
    expect(container.querySelector('[data-live="true"]')).not.toBeNull();
  });

  it("a reconnecting stream degrades to a snapshot tile with its last event and a still sigil", async () => {
    streamMock.mockReturnValue({
      feed: "ran a check",
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
      feed: undefined,
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
    expect(container.textContent).toMatch(/AGENT [A-Z]+-[0-9A-F]{4}/);
  });

  it("distinguishes same-named fleet links by their agent callsigns", () => {
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { getAllByRole } = render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(FleetTile, { fleet: fleet({ id: "fleet-alpha" }), workspaceId: "ws_1" }),
          React.createElement(FleetTile, { fleet: fleet({ id: "fleet-bravo" }), workspaceId: "ws_1" }),
        ),
      ),
    );
    const labels = getAllByRole("link").map((link) => link.getAttribute("aria-label"));
    expect(labels[0]).toMatch(/^Manage fleet: alpha — AGENT [A-Z]+-[0-9A-F]{4} — active$/);
    expect(labels[1]).toMatch(/^Manage fleet: alpha — AGENT [A-Z]+-[0-9A-F]{4} — active$/);
    expect(labels[0]).not.toBe(labels[1]);
  });

  it("draws the sigil inside its box, mirrored across the centre", () => {
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container } = renderTile(
      fleet({ id: "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee" }),
    );
    const cells = Array.from(
      container.querySelectorAll('svg rect[fill="currentColor"]'),
      (cell) => ({
        x: Number(cell.getAttribute("x")),
        y: Number(cell.getAttribute("y")),
        width: Number(cell.getAttribute("width")),
        height: Number(cell.getAttribute("height")),
      }),
    );
    expect(cells.length).toBeGreaterThan(0);
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
      feed: undefined,
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
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      helloReceived: false,
      isLive: true,
      catchingUp: false,
    });
    const { container, getByText } = renderTile(fleet());
    expect(container.querySelector('[data-fleet-sigil][data-live="true"]')).toBeNull();
    expect(getByText(FLEET_NO_LIVE_ACTIVITY_COPY)).toBeTruthy();
  });

  it("the footer is the snapshot the stream assigned, over the server render", () => {
    // The stream ASSIGNS what the last frame said; the tile never adds to the
    // figures it was rendered with, so the server's 7 events and $1.20 are
    // replaced by the frame's 9 and $2.50 outright.
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
      counters: { eventsProcessed: 9, spentNanos: 2_500_000_000 },
    });
    const { getByText } = renderTile(fleet());
    expect(getByText("spent").textContent).toBe("$2.50 spent");
    expect(getByText("events").textContent).toBe("9 events");
  });

  it("a fresh exact REST count wins over a retained stream snapshot", () => {
    streamMock.mockReturnValue({
      feed: undefined, connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true, isLive: true, catchingUp: false,
      counters: { eventsProcessed: 9, spentNanos: 2_000_000_000 },
    });
    const { getByText } = renderTile(fleet({ budget_used_nanos: 9007199254740993n, events_processed: 9007199254740993n }));
    expect(getByText("spent").textContent).toBe("$9007199.25 spent");
    expect(getByText("events").textContent).toBe("9007199254740993 events");
  });

  it("a tile absent from the server hello set renders snapshot, not live", () => {
    streamMock.mockReturnValue({
      feed: undefined,
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

  it("leads with the fleet name and keeps the agent identity secondary", () => {
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: false,
    });
    const { container } = renderTile(fleet());
    const agent = container.querySelector("[data-agent-name]");
    expect(agent?.textContent).toBe(agentDisplayName("flt_1"));
    expect(container.querySelector("[data-fleet-status]")?.textContent).toBe("Active");
    // The status sits on the agent's own line, and the activity is the next.
    expect((container.querySelector("[data-fleet-name]")?.compareDocumentPosition(agent!) ?? 0) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(container.querySelector("[data-tile-activity]")?.textContent).toBe(FLEET_WAITING_COPY);
    expect(container.querySelector("[data-fleet-name]")?.textContent).toBe("alpha");
    expect(container.textContent).not.toContain("Runs in a loop");
  });

  it("a server drop signal surfaces catching up", () => {
    streamMock.mockReturnValue({
      feed: undefined,
      connectionStatus: CONNECTION_STATUS.LIVE,
      helloReceived: true,
      isLive: true,
      catchingUp: true,
    });
    const { getByText } = renderTile(fleet());
    expect(getByText(TILE_CATCHING_UP_EYEBROW)).toBeTruthy();
  });
});
