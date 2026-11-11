import { ev, mockStream, renderThread, useFleetEventStreamMock, WS, ZID } from "./fleet-thread/harness";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { convertEvent, CONNECTION_STATUS, type FleetEvent } from "@/components/domain/useFleetEventStream";
import { ApprovalActivityElement } from "@/components/domain/FleetActivityElements";
import { FleetChatElementView } from "@/components/domain/FleetChatElements";
import { GROUP_META } from "@/components/domain/useFleetThreadEntries";
import { ACTOR, OUTCOME } from "@/lib/events/event-summary";
import type { ActivityElementData } from "@/components/domain/fleet-chat-elements";

const APPROVAL: ActivityElementData = {
  eventId: "access-request", actor: ACTOR.GATE_BLOCKED, status: "gate_blocked",
  createdAt: new Date("2026-11-11T09:00:00Z"), headline: "Slack access needs approval",
  outcome: OUTCOME.WAITING_APPROVAL, failed: false, details: {kind: "none"}, source: null,
};

describe("registered fleet chat elements", () => {
  it("renders mixed recorded outcomes through the real transcript", async () => {
    mockStream([
      ev({id: "operator", role: "user", actor: "steer:private-person", text: "Review the release", reply: "Checks are ready."}),
      ev({id: "slack", role: "system", actor: "slack:member", text: "Message received",
        custom: {requestJson: JSON.stringify({message: "Review 東京 🦉", channel_id: "private-channel"})}}),
      ev({id: "wakeup", role: "system", actor: "cron", text: "Scheduled wake-up"}),
      ev({id: "approval", role: "system", actor: ACTOR.GATE_BLOCKED, text: APPROVAL.headline, status: "gate_blocked",
        outcome: OUTCOME.WAITING_APPROVAL,
        custom: {requestJson: JSON.stringify({fleet_id: "forged-fleet", workspace_id: "forged-workspace", approval_id: "forged-approval"})}}),
      ...["first", "second"].map(id => ev({id, role: "system", actor: "webhook:github", text: "Release check received", status: "received"})),
    ]);
    const {container} = renderThread();
    expect(screen.getByText("Review the release")).toBeTruthy();
    expect(screen.getByText("Checks are ready.")).toBeTruthy();
    expect(container.querySelectorAll('[data-role="user"]')).toHaveLength(1);
    expect(container.querySelectorAll('[data-role="assistant"]')).toHaveLength(1);
    expect(screen.getByText("Scheduled wake-up")).toBeTruthy();
    expect(screen.getByRole("button", {name: "Run details"})).toBeTruthy();
    expect(screen.getByRole("link", {name: "View approvals"}).getAttribute("href"))
      .toBe(`/w/${WS}/approvals?fleetId=${ZID}`);
    expect(container.textContent).not.toContain("private-person");
    expect(container.textContent).not.toContain("forged-approval");
    await act(async () => { fireEvent.click(screen.getByRole("button", {name: "View message"})); });
    expect(screen.getByText("Review 東京 🦉")).toBeTruthy();
    expect(container.textContent).not.toContain("private-channel");
    await act(async () => { fireEvent.click(screen.getByRole("button", {name: /×2/, expanded: false})); });
    expect(screen.getAllByText("Release check received")).toHaveLength(3);
  });

  it.each([
    ["empty", []],
    ["non-array", {component: "RunCommand", command: "do-not-execute"}],
    ["incomplete member", [{id: "broken", text: "do-not-expose"}]],
    ["operator member", [ev({role: "user", actor: "steer:private-person", text: "do-not-expose"})]],
    ["invalid instant", [ev({role: "system", actor: "cron", createdAt: new Date(NaN)})]],
    ["duplicate identities", ["one", "two"].map(() => ev({id: "duplicate", role: "system", actor: "cron"}))],
  ])("keeps %s group metadata safe and visible", (_label, members) => {
    mockMalformedGroup(members);
    const {container} = renderThread();
    expect(screen.getByText("Invalid grouped activity")).toBeTruthy();
    expect(screen.getByText("Some activity details couldn’t be loaded.")).toBeTruthy();
    expect(container.querySelector('[data-group="true"]')).toBeNull();
    expect(container.querySelector("pre, script")).toBeNull();
    expect(container.textContent).not.toMatch(/do-not-execute|do-not-expose/);
  });

  it("retains the source reference in a collapsed activity group", () => {
    mockStream(["first", "second"].map(id => ev({id, role: "system", actor: "webhook:github",
      text: "opened · agentsfleet/release#42 — Review release",
      custom: {requestJson: JSON.stringify({repo: "agentsfleet/release", number: 42, url: "https://github.com/agentsfleet/release/pull/42"})},
    })));
    renderThread();
    expect(screen.getByRole("button", {name: /×2/, expanded: false}).textContent)
      .toContain("opened · agentsfleet/release#42 — Review release");
  });

  it("keeps an approval without thread scope readable without offering a destination", () => {
    render(<ApprovalActivityElement data={APPROVAL} />);
    expect(screen.getByText(APPROVAL.headline)).toBeTruthy();
    expect(screen.queryByRole("link", {name: "View approvals"})).toBeNull();
    expect(screen.queryByRole("button", {name: /approve|deny/i})).toBeNull();
  });

  it("shows recorded text and outcome with the default activity renderer", () => {
    const {container} = render(<FleetChatElementView element={{kind: "default-activity", data: {...APPROVAL, headline: "New integration update", notice: null}}} />);
    expect(screen.getByText("New integration update")).toBeTruthy();
    expect(screen.getByText(OUTCOME.WAITING_APPROVAL)).toBeTruthy();
    expect(screen.queryByText(/unsupported|not supported/i)).toBeNull();
    expect(container.querySelector("script, iframe, button")).toBeNull();
  });
});

function mockMalformedGroup(members: unknown) {
  const event = ev({id: "bad-group", role: "system", actor: "webhook:github", text: "Invalid grouped activity"});
  useFleetEventStreamMock.mockReturnValue({
    events: [event], connectionStatus: CONNECTION_STATUS.LIVE, isRunning: false,
    appendOptimistic: vi.fn(), reconcileOptimistic: vi.fn(), discardOptimistic: vi.fn(), retryConnection: vi.fn(),
    convertEvent: (record: FleetEvent) => ({
      ...convertEvent(record), metadata: {custom: {[GROUP_META.MEMBERS]: members}},
    }),
  });
}
