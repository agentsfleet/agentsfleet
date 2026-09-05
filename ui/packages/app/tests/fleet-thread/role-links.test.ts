import { ev, mockStream, renderThread } from "./harness";
import { describe, expect, it } from "vitest";
import { act, screen } from "@testing-library/react";
import { OUTCOME, outcomeFor } from "@/lib/events/event-summary";
import { FleetThread } from "@/components/domain/FleetThread";

describe("FleetThread — role rendering: links and guidance", () => {
  it("links the repository reference out when the payload carries a URL", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
    ]);
    const { container } = renderThread();

    expect(
      screen.getByRole("link", { name: "agentsfleet/agentsfleet#541" }),
    ).toBeTruthy();
    expect(screen.getByText("opened")).toBeTruthy();
    const link = container.querySelector('a[href^="https://github.com"]');
    expect(link).toBeTruthy();
    expect(link?.getAttribute("rel")).toContain("noopener");
    expect(link?.className).toMatch(/\bmin-h-11\b/);
    expect(link?.className).toMatch(/\bsm:min-h-6\b/);
  });

  it("keeps an activity headline readable when its linked source reference is removed", () => {
    mockStream([
      ev({
        id: "headline_1",
        role: "system",
        actor: "webhook:github",
        text: "opened agentsfleet/agentsfleet#541 — Fix routing",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
      ev({
        id: "headline_2",
        role: "system",
        actor: "webhook:github",
        text: "edited · agentsfleet/agentsfleet#542 — Add evidence",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"edited","repo":"agentsfleet/agentsfleet","number":542,"url":"https://github.com/agentsfleet/agentsfleet/pull/542"}',
        },
      }),
      ev({
        id: "headline_3",
        role: "system",
        actor: "webhook:github",
        text: "closed agentsfleet/agentsfleet#543 after review",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"closed","repo":"agentsfleet/agentsfleet","number":543,"url":"https://github.com/agentsfleet/agentsfleet/pull/543"}',
        },
      }),
      ev({
        id: "headline_4",
        role: "system",
        actor: "webhook:github",
        text: "synchronized agentsfleet/agentsfleet#544",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"synchronized","repo":"agentsfleet/agentsfleet","number":544,"url":"https://github.com/agentsfleet/agentsfleet/pull/544"}',
        },
      }),
    ]);
    renderThread();

    expect(screen.getByText("opened · Fix routing")).toBeTruthy();
    expect(screen.getByText("edited · Add evidence")).toBeTruthy();
    expect(screen.getByText("closed after review")).toBeTruthy();
    expect(screen.getByText("synchronized")).toBeTruthy();
  });

  it("retains an activity headline that does not repeat its linked reference", () => {
    mockStream([
      ev({
        id: "reference_absent",
        role: "system",
        actor: "webhook:github",
        text: "GitHub delivery received",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
    ]);
    renderThread();

    expect(screen.getByText("GitHub delivery received")).toBeTruthy();
  });

  it("renders no link for a payload whose URL is not an absolute http(s) address", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        // A script URL and a relative path are both refused: one executes,
        // the other resolves against the console's own origin.
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"javascript:alert(1)"}',
        },
      }),
    ]);
    const { container } = renderThread();

    expect(
      screen.getByText("opened · agentsfleet/agentsfleet#541"),
    ).toBeTruthy();
    expect(container.querySelector("a[href]")).toBeNull();
  });

  it("names the failing check and what to do about it on a startup failure", () => {
    const cause =
      "startup check 'instructions' failed: no instructions configured";
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: outcomeFor({
          status: "fleet_error",
          failure_label: "startup_posture",
          failure_detail: cause,
        }),
        failureLabel: "startup_posture",
      }),
    ]);
    renderThread();
    // The cause reaches the row, not just the class sentence ...
    expect(screen.getByText(new RegExp(cause))).toBeTruthy();
    // ... without repeating repair guidance in the transcript.
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("offers no guidance for a failure class the operator cannot act on", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: outcomeFor({
          status: "fleet_error",
          failure_label: "oom_kill",
          failure_detail: null,
        }),
        failureLabel: "oom_kill",
      }),
    ]);
    renderThread();
    expect(
      screen.queryByText(
        "Tell the fleet what to do in its instructions, then retry.",
      ),
    ).toBeNull();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("lets the fleet's own reply stand instead of canned guidance", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "I recovered on the retry and reviewed the change.",
        status: "fleet_error",
        outcome: OUTCOME.FAILED,
        failureLabel: "startup_posture",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/I recovered on the retry/)).toBeTruthy();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("never leaks the operator account identifier into the transcript", () => {
    const accountId = "user_3gkbgxjnujsxbdxttcwcslpc87k";
    mockStream([
      ev({
        role: "user",
        actor: `steer:${accountId}`,
        text: "morning health check",
      }),
    ]);
    const { container } = renderThread();
    expect(screen.getByText(/morning health check/)).toBeTruthy();
    expect(screen.queryByText("Operator")).toBeNull();
    expect(screen.queryByText("OP")).toBeNull();
    expect(container.textContent).not.toContain(accountId);
  });
});
