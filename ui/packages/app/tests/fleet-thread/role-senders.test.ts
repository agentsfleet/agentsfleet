import { FLEET_NAME, SUBJECT, WS, ZID, ev, mockStream } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { FleetThread } from "@/components/domain/FleetThread";
import { ACTOR } from "@/lib/events/event-summary";
import { OWN_SENDER } from "@/lib/events/sender-names";

// The harness signs in as this subject, and a signed-in user outranks the
// server-rendered viewer.
const VIEWER = SUBJECT;
const BOB = `${ACTOR.STEER_PREFIX}user_bob`;
const SENDER_LINE = '[data-testid="fleet-message-sender"]';

function renderShared() {
  return render(
    React.createElement(FleetThread, {
      workspaceId: WS,
      fleetId: ZID,
      senderLabel: FLEET_NAME,
      initial: [],
      viewer: VIEWER,
      senderNames: [{ actor: BOB, name: "Bob" }],
    }),
  );
}

describe("FleetThread — who sent each turn in a shared thread", () => {
  it("shows a teammate's name above their message", () => {
    mockStream([ev({ id: "e1", role: "user", actor: BOB, text: "check the tests" })]);
    const { container } = renderShared();
    const row = container.querySelector('[data-role="user"]');
    expect(row?.querySelector(SENDER_LINE)?.textContent).toBe("Bob");
    // Read once: the visible name is hidden from assistive tech, which hears
    // the bubble's own prefix.
    expect(row?.querySelector(SENDER_LINE)?.getAttribute("aria-hidden")).toBe("true");
    expect(row?.querySelector(".sr-only")?.textContent).toBe("Bob: ");
    expect(screen.getByText("check the tests")).toBeTruthy();
  });

  it("keeps the viewer's own message free of a sender line", () => {
    mockStream([ev({ id: "e1", role: "user", actor: `${ACTOR.STEER_PREFIX}${VIEWER}`, text: "ship it" })]);
    const { container } = renderShared();
    const row = container.querySelector('[data-role="user"]');
    expect(row?.querySelector(SENDER_LINE)).toBeNull();
    expect(row?.querySelector(".sr-only")?.textContent).toBe(`${OWN_SENDER}: `);
  });
});
