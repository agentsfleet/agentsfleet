import { ev, mockStream, renderThread } from "./harness";
import { describe, expect, it } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";
import { OUTCOME } from "@/lib/events/event-summary";
import { FleetThread } from "@/components/domain/FleetThread";

describe("FleetThread — role rendering: activity groups and traces", () => {
  it("collapses a run of identical deliveries into one row that opens on demand", async () => {
    const burst = Array.from({ length: 15 }, (_, i) =>
      ev({
        id: `burst_${i}`,
        role: "system",
        actor: "webhook:github",
        text: "edited · agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check — no instructions configured",
        failureLabel: "startup_posture",
      }),
    );
    mockStream(burst);
    const { container } = renderThread();

    // Fifteen deliveries, one row — and the count is stated, not implied.
    expect(screen.getByTestId("group-count").textContent).toBe("×15");
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(0);

    // The count is a summary the operator can always check.
    const toggle = screen.getByRole("button", { expanded: false });
    await act(async () => {
      fireEvent.click(toggle);
    });
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(
      15,
    );
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("keeps each grouped delivery's payload reachable after expansion", async () => {
    const payload =
      '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541}';
    mockStream([
      ev({
        id: "payload_1",
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: { requestJson: payload },
      }),
      ev({
        id: "payload_2",
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: { requestJson: payload },
      }),
    ]);
    renderThread();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { expanded: false }));
    });
    expect(screen.getAllByText("Details")).toHaveLength(2);
  });

  it("keeps reply-bearing activity as separate conversation turns", async () => {
    // Reply-bearing events are not grouped: each trigger and response needs
    // its own assistant-ui root, even when the trigger text is identical.
    const member = (id: string, reply: string, requestJson?: string) =>
      ev({
        id,
        role: "system",
        actor: "webhook:github",
        text: "edited #541",
        reply,
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        ...(requestJson ? { custom: { requestJson } } : {}),
      });
    mockStream([
      member("g1", "", '{"action":"opened","repo":"o/r","number":1}'),
      member("g2", "reviewed it", undefined),
      member("g3", "reviewed it", undefined),
    ]);
    const { container } = renderThread();

    expect(screen.queryByTestId("group-count")).toBeNull();
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(3);
    expect(container.querySelectorAll('[data-role="assistant"]')).toHaveLength(
      2,
    );
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"repo":\s*"o\/r"/)).toBeTruthy();
  });

  it("links out from an activity delivery that carries only a run URL", () => {
    // A completed-run payload has a link but no repository reference, so the
    // annotation renders a generic source action.
    mockStream([
      ev({
        id: "run1",
        role: "system",
        actor: "webhook:github",
        text: "ci finished",
        reply: "",
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        custom: {
          requestJson:
            '{"workflow_name":"ci","conclusion":"success","repo":"o/r","run_url":"https://ci.example.test/1"}',
        },
      }),
    ]);
    const { container } = renderThread();
    const link = container.querySelector('a[href^="https://ci.example.test"]');
    expect(link).toBeTruthy();
    expect(container.querySelector('[data-slot="badge"]')).toBeNull();
  });

  it("shows a startup failure without duplicating the operator payload", () => {
    // One durable turn: the operator steered, the run failed a startup check.
    // The reply states the outcome and points at the fix.
    mockStream([
      ev({
        id: "steer_fail",
        role: "user",
        actor: "steer:user_abc",
        text: "deploy the review guidelines",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
        custom: { requestJson: '{"message":"deploy the review guidelines"}' },
      }),
    ]);
    renderThread();
    expect(screen.queryByText("Details")).toBeNull();
    const failure = screen.getByText(
      /This fleet needs instructions before it can respond/,
    );
    expect(failure.className).toMatch(/text-label/);
    expect(failure.className).toMatch(/font-medium/);
    expect(failure.className).toMatch(/leading-label/);
    expect(failure.className).toMatch(/text-foreground/);
    expect(failure.className).not.toMatch(/text-warning/);
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("streams a fleet reply that has begun but not finished", () => {
    // A reply mid-stream: status received, partial text already accumulated.
    // The reply body shows with the streaming cursor, not the working dots.
    mockStream([
      ev({
        id: "sr",
        role: "assistant",
        actor: "fleet",
        text: "",
        reply: "Half a thought",
        status: "received",
        outcome: OUTCOME.WORKING,
      }),
    ]);
    renderThread();
    expect(screen.getByText(/Half a thought/)).toBeTruthy();
    expect(screen.getByLabelText("streaming")).toBeTruthy();
  });

  it("breaks a group when the operator speaks mid-burst", () => {
    const activity = (id: string) =>
      ev({
        id,
        role: "system",
        actor: "webhook:github",
        text: "edited #541",
        reply: "",
        status: "fleet_error",
      });
    mockStream([
      activity("a1"),
      activity("a2"),
      ev({ role: "user", actor: "steer:user_abc", text: "what is going on?" }),
      activity("b1"),
      activity("b2"),
    ]);
    const { container } = renderThread();

    // Two groups, and the operator's question still reads as their own row.
    expect(container.querySelectorAll('[data-group="true"]')).toHaveLength(2);
    expect(screen.getByText("what is going on?")).toBeTruthy();
  });

  it("renders an integration delivery as a flat trace with reachable payload", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541 — Fix routing",
        reply: "",
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541}',
        },
      }),
    ]);
    const { container } = renderThread();

    // One integration record, not a duplicated source row plus outcome row.
    const tick = container.querySelector('[data-compact="true"]');
    expect(tick).toBeTruthy();
    expect(container.querySelectorAll('[data-role="system"]')).toHaveLength(1);
    expect(tick?.textContent).toContain("agentsfleet/agentsfleet#541");
    // The outcome is readable beneath the source context, not another row.
    expect(screen.getByText(OUTCOME.NO_REPLY).className).toMatch(
      /text-muted-foreground/,
    );
    // Disclosure remains reachable beside the source evidence.
    expect(screen.getByText("Details")).toBeTruthy();
  });

  it("keeps conversation rows distinct without sender chrome", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "deploy staging",
        reply: "",
      }),
      ev({ role: "assistant", actor: "fleet", text: "", reply: "Deployed." }),
    ]);
    const { container } = renderThread();

    expect(container.querySelector('[data-compact="true"]')).toBeNull();
    expect(container.querySelectorAll("[data-chip]")).toHaveLength(0);
    expect(screen.getByText("deploy staging")).toBeTruthy();
    expect(screen.getByText("Deployed.")).toBeTruthy();
  });
});
