import { FLEET_NAME, SUBJECT, WS, ZID, ev, mockStream } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { FleetThread } from "@/components/domain/FleetThread";
import { ACTOR } from "@/lib/events/event-summary";
import { OWN_SENDER } from "@/lib/events/sender-names";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";

// The harness signs in as this subject, and a signed-in user outranks the
// server-rendered viewer.
const VIEWER = SUBJECT;
const BOB = `${ACTOR.STEER_PREFIX}user_bob`;
const BOB_NAME = "Bob";
const TEAMMATE_SAYS = "check the tests";
const QUEUED_SAYS = "rerun the suite";
const SENDER_LINE = '[data-testid="fleet-message-sender"]';
// The library's role for a person's turn; the row carries it as `data-role`.
const PERSON = "user" as const;
const USER_ROW = `[data-role="${PERSON}"]`;
const REPLY_ROW = '[data-role="assistant"]';

function renderShared() {
  return render(
    React.createElement(FleetThread, {
      workspaceId: WS,
      fleetId: ZID,
      senderLabel: FLEET_NAME,
      initial: [],
      viewer: VIEWER,
      senderNames: [{ actor: BOB, name: BOB_NAME }],
    }),
  );
}

describe("FleetThread — who sent each turn in a shared thread", () => {
  it("shows a teammate's name above their message", () => {
    mockStream([ev({ id: "e1", role: PERSON, actor: BOB, text: TEAMMATE_SAYS })]);
    const { container } = renderShared();
    const row = container.querySelector(USER_ROW);
    expect(row?.querySelector(SENDER_LINE)?.textContent).toBe(BOB_NAME);
    // Read once: the visible name is hidden from assistive tech, which hears
    // the bubble's own prefix.
    expect(row?.querySelector(SENDER_LINE)?.getAttribute("aria-hidden")).toBe("true");
    expect(row?.querySelector(".sr-only")?.textContent).toBe(`${BOB_NAME}: `);
    expect(screen.getByText(TEAMMATE_SAYS)).toBeTruthy();
  });

  it("keeps the viewer's own message free of a sender line", () => {
    mockStream([ev({ id: "e1", role: PERSON, actor: `${ACTOR.STEER_PREFIX}${VIEWER}`, text: "ship it" })]);
    const { container } = renderShared();
    const row = container.querySelector(USER_ROW);
    expect(row?.querySelector(SENDER_LINE)).toBeNull();
    expect(row?.querySelector(".sr-only")?.textContent).toBe(`${OWN_SENDER}: `);
  });

  // The server says a turn waits for a runner with its status alone; no
  // client-side flag marks a teammate's turn as queued.
  it("should name a teammate above their bubble and show the reply as queued when their turn waits for a runner", () => {
    mockStream([ev({ id: "e1", role: PERSON, actor: BOB, text: QUEUED_SAYS, status: AGENTSFLEET_EVENT_STATUS.QUEUED })]);
    const { container } = renderShared();
    const row = container.querySelector<HTMLElement>(USER_ROW);
    const sender = row?.querySelector(SENDER_LINE) ?? null;
    if (row === null || sender === null) throw new Error("no named teammate row rendered");
    const bubble = within(row).getByText(QUEUED_SAYS);
    expect(sender.textContent).toBe(BOB_NAME);
    expect(sender.compareDocumentPosition(bubble) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const reply = container.querySelector<HTMLElement>(REPLY_ROW);
    if (reply === null) throw new Error("no reply row rendered");
    expect(within(reply).getByRole("status", { name: "Queued" })).toBeTruthy();
    expect(within(reply).queryByRole("status", { name: "Working" })).toBeNull();
  });
});
