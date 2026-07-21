import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FleetMessageRow, FleetNameProvider, ROW_TONE, useFleetName } from "./FleetMessageRow";

const AT = new Date(Date.UTC(2026, 6, 21, 10, 42, 17));

afterEach(() => cleanup());

function renderRow(overrides: Partial<Parameters<typeof FleetMessageRow>[0]> = {}) {
  return render(
    <FleetMessageRow
      sender="Operator"
      createdAt={AT}
      tone={ROW_TONE.OPERATOR}
      messageRole="user"
      {...overrides}
    >
      {overrides.children ?? "please review the change"}
    </FleetMessageRow>,
  );
}

describe("FleetMessageRow", () => {
  it("renders the approved shape: chip, sender, timestamp, body, separator", () => {
    const { container } = renderRow();
    const row = container.querySelector('[data-role="user"]') as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.querySelector('[data-chip="operator"]')?.textContent).toBe("OP");
    expect(screen.getByText("Operator")).toBeTruthy();
    expect(screen.getByText("please review the change")).toBeTruthy();
    expect(row.className).toMatch(/border-b/);
  });

  it("carries the exact instant in the timestamp, whatever the visible format", () => {
    const { container } = renderRow();
    expect(container.querySelector("time")?.getAttribute("dateTime")).toBe(AT.toISOString());
  });

  it("pushes the timestamp to the far edge, away from the sender", () => {
    const { container } = renderRow();
    const header = container.querySelector("time")?.parentElement as HTMLElement;
    expect(header.className).toMatch(/items-baseline/);
    // The spacer between sender and time is what holds them apart.
    expect(header.querySelector(".flex-1")).toBeTruthy();
  });

  it("keeps a long body inside its own row rather than widening the page", () => {
    const { container } = renderRow({ children: "x".repeat(600) });
    const body = container.querySelector(".break-words") as HTMLElement;
    expect(body).toBeTruthy();
    expect(body.className).toMatch(/min-w-0/);
  });

  it("tones the chip per role without changing the row's skeleton", () => {
    const { container: fleet } = renderRow({ tone: ROW_TONE.FLEET, sender: "pr-reviewer" });
    expect(fleet.querySelector('[data-chip="fleet"]')).toBeTruthy();
    cleanup();
    const { container: event } = renderRow({ tone: ROW_TONE.EVENT, sender: "github-app" });
    expect(event.querySelector('[data-chip="event"]')?.textContent).toBe("GA");
  });

  it("dims a sending row and marks a failed one for the renderer", () => {
    const { container } = renderRow({ dimmed: true, failed: true });
    const row = container.querySelector('[data-role="user"]') as HTMLElement;
    expect(row.getAttribute("data-optimistic")).toBe("true");
    expect(row.getAttribute("data-failed")).toBe("true");
    expect(row.className).toMatch(/opacity-60/);
  });

  it("renders an annotation beside the sender when one is given", () => {
    renderRow({ annotation: <span>sending</span> });
    expect(screen.getByText("sending")).toBeTruthy();
  });
});

describe("FleetNameProvider", () => {
  function Probe() {
    return <span>{useFleetName() || "(none)"}</span>;
  }

  it("carries the console's fleet name to rows the thread primitive renders", () => {
    render(
      <FleetNameProvider fleetName="github-pr-reviewer">
        <Probe />
      </FleetNameProvider>,
    );
    expect(screen.getByText("github-pr-reviewer")).toBeTruthy();
  });

  it("reads as absent outside a provider rather than throwing", () => {
    render(<Probe />);
    expect(screen.getByText("(none)")).toBeTruthy();
  });
});
