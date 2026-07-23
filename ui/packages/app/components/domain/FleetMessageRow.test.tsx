import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import {
  FleetActivityRow,
  FleetGroupRow,
  FleetMessageRow,
  FleetNameProvider,
  ROW_TONE,
  useFleetName,
} from "./FleetMessageRow";

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
  it("renders an operator turn in a right-aligned bounded surface", () => {
    const { container } = renderRow();
    const row = container.querySelector('[data-role="user"]') as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.querySelector('[data-chip="operator"]')?.textContent).toBe("OP");
    expect(screen.getByText("Operator")).toBeTruthy();
    expect(screen.getByText("please review the change")).toBeTruthy();
    const surface = row.querySelector("[data-dashboard-row]") as HTMLElement;
    expect(surface.className).toMatch(/max-w-4xl/);
    expect(surface.className).toMatch(/rounded-lg/);
    expect(surface.className).toMatch(/border/);
  });

  it("keeps fleet replies left aligned without an operator bubble", () => {
    const { container } = renderRow({
      tone: ROW_TONE.FLEET,
      sender: "pr-reviewer",
      messageRole: "assistant",
    });
    const row = container.querySelector('[data-role="assistant"]') as HTMLElement;
    const surface = row.querySelector("[data-dashboard-row]") as HTMLElement;
    expect(surface.className).toMatch(/max-w-5xl/);
    expect(surface.className).not.toMatch(/rounded-lg/);
  });

  it("carries the exact instant in the timestamp, whatever the visible format", () => {
    const { container } = renderRow();
    expect(container.querySelector("time")?.getAttribute("dateTime")).toBe(AT.toISOString());
  });

  it("pushes the timestamp to the far edge, away from the sender", () => {
    const { container } = renderRow();
    const header = container.querySelector("time")?.parentElement as HTMLElement;
    expect(header.className).toMatch(/ml-auto/);
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

describe("FleetActivityRow", () => {
  it("keeps integration metadata together and separates a failed outcome", () => {
    const { container } = render(
      <FleetActivityRow
        sender="GitHub App"
        headline="agentsfleet/agentsfleet#546 was edited"
        createdAt={AT}
        annotation={<span>EDITED</span>}
        outcome="This fleet needs instructions before it can respond."
        failed
        messageRole="system"
      >
        <span>extended delivery context</span>
      </FleetActivityRow>,
    );
    const row = container.querySelector('[data-role="system"]') as HTMLElement;
    const card = row as HTMLElement;
    const outcome = screen.getByText("This fleet needs instructions before it can respond.");

    expect(card.className).toMatch(/w-full/);
    expect(card.className).toMatch(/border-b/);
    expect(screen.getByText("EDITED")).toBeTruthy();
    expect(row.querySelector("time")?.getAttribute("dateTime")).toBe(AT.toISOString());
    expect(outcome.className).toMatch(/text-destructive/);
    expect(screen.getByText("Details")).toBeTruthy();
  });

  it("shows guidance even when the integration has no outcome text", () => {
    render(
      <FleetActivityRow
        sender="GitHub App"
        headline="Webhook received"
        createdAt={AT}
        guidance={<span>Connect the source, then retry.</span>}
        messageRole="system"
      />,
    );

    expect(screen.getByText("Connect the source, then retry.")).toBeTruthy();
  });
});

describe("FleetGroupRow", () => {
  function renderGroup(expanded: boolean, outcome?: string) {
    const onToggle = vi.fn();
    const view = render(
      <FleetGroupRow
        sender="GitHub App"
        headline="Webhook received"
        outcome={outcome}
        count={2}
        first={AT}
        last={new Date(AT.getTime() + 60_000)}
        expanded={expanded}
        onToggle={onToggle}
      >
        <span>Individual delivery</span>
      </FleetGroupRow>,
    );
    return { onToggle, ...view };
  }

  it("toggles a collapsed group and omits the outcome when none exists", () => {
    const { onToggle, container } = renderGroup(false);
    expect(container.textContent).not.toContain("No outcome");
    fireEvent.click(screen.getByRole("button", { name: /Webhook received/ }));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("toggles an expanded group closed", () => {
    const { onToggle } = renderGroup(true, "No outcome");
    fireEvent.click(screen.getByRole("button", { name: /Webhook received/ }));
    expect(onToggle).toHaveBeenCalledTimes(1);
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
