import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import {
  FleetActivityRow,
  FleetGroupRow,
  FleetMessageRow,
  SenderLabelProvider,
  ROW_TONE,
  useSenderLabel,
} from "./FleetMessageRow";
import { RUNNER_REFUSAL_SENTENCE } from "@/lib/events/event-summary";

// Dimming rides `data-optimistic`; the row itself carries no fade class.
const FADED_ROW_CLASS = /opacity-60/;
const AT = new Date(Date.UTC(2026, 6, 21, 10, 42, 17));

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function renderRow(
  overrides: Partial<Parameters<typeof FleetMessageRow>[0]> = {},
) {
  const tone = overrides.tone ?? ROW_TONE.OPERATOR;
  const sender =
    overrides.sender ?? (tone === ROW_TONE.FLEET ? "Fleet" : "Operator");
  return render(
    <FleetMessageRow
      sender={sender}
      tone={tone}
      messageRole="user"
      {...overrides}
    >
      {overrides.children ?? "please review the change"}
    </FleetMessageRow>,
  );
}

describe("FleetMessageRow", () => {
  it("shows only the operator message in a right-aligned bubble", () => {
    const { container } = renderRow();
    const row = container.querySelector('[data-role="user"]') as HTMLElement;
    const bubble = screen.getByText("please review the change");

    expect(row.querySelector(".justify-end")).toBeTruthy();
    expect(row.querySelector("[data-chip]")).toBeNull();
    expect(row.querySelector("time")).toBeNull();
    expect(bubble.className).toMatch(/rounded-lg/);
    expect(bubble.className).toMatch(/rounded-br-sm/);
    expect(bubble.className).toMatch(/bg-accent/);
    expect(
      screen.getByText("Operator:", { selector: ".sr-only" }),
    ).toBeTruthy();
  });

  it("retains a non-operator sender for assistive technology", () => {
    renderRow({ sender: "API" });
    expect(screen.getByText("API:", { selector: ".sr-only" })).toBeTruthy();
    expect(screen.queryByText("Operator:", { selector: ".sr-only" })).toBeNull();
  });

  it("renders a fleet reply as open text without conversation chrome", () => {
    const { container } = renderRow({
      tone: ROW_TONE.FLEET,
      messageRole: "assistant",
    });
    const row = container.querySelector(
      '[data-role="assistant"]',
    ) as HTMLElement;
    const reply = screen.getByText("please review the change");

    expect(row.querySelector(".justify-start")).toBeTruthy();
    expect(row.querySelector("[data-chip]")).toBeNull();
    expect(row.querySelector("time")).toBeNull();
    expect(reply.className).not.toMatch(/rounded/);
    expect(reply.className).not.toMatch(/\bborder\b/);
    expect(reply.className).not.toMatch(/\bbg-/);
    expect(screen.getByText("Fleet:", { selector: ".sr-only" })).toBeTruthy();
  });

  it("dims a sending row and marks a failed one for the renderer", () => {
    const { container } = renderRow({ dimmed: true, failed: true });
    const row = container.querySelector('[data-role="user"]') as HTMLElement;
    expect(row.getAttribute("data-optimistic")).toBe("true");
    expect(row.getAttribute("data-failed")).toBe("true");
    expect(row.className).not.toMatch(FADED_ROW_CLASS);
  });
});

describe("FleetActivityRow", () => {
  it("keeps integration metadata together and marks a failed outcome destructive", () => {
    const { container } = render(
      <FleetActivityRow
        sender="GitHub App"
        headline="agentsfleet/agentsfleet#546 was edited"
        createdAt={AT}
        annotation={<span>EDITED</span>}
        outcome={RUNNER_REFUSAL_SENTENCE}
        failed
        messageRole="system"
      >
        <span>extended delivery context</span>
      </FleetActivityRow>,
    );
    const row = container.querySelector('[data-role="system"]') as HTMLElement;
    const outcome = screen
      .getByText(RUNNER_REFUSAL_SENTENCE)
      .closest("p") as HTMLElement;
    const time = row.querySelector("time") as HTMLTimeElement;
    const accessibleTime = screen.getByText(/^Occurred /, {
      selector: ".sr-only",
    });

    expect(screen.getByText("agentsfleet/agentsfleet#546 was edited")).toBeTruthy();
    expect(screen.getByText("EDITED")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Details" })).toBeTruthy();
    expect(time.dateTime).toBe(AT.toISOString());
    expect(time.getAttribute("aria-hidden")).toBe("true");
    expect(time.textContent).toMatch(/ago$/);
    expect(time.title).not.toBe("");
    expect(accessibleTime.textContent).toContain(time.title);
    expect(outcome.querySelector("svg")?.getAttribute("class")).toMatch(
      /text-destructive/,
    );
  });

  it("refreshes an idle activity timestamp as wall time advances", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(AT.getTime() + 2 * 60_000));
    const { container } = render(
      <SenderLabelProvider senderLabel="github-pr-reviewer">
        <FleetActivityRow
          sender="GitHub App"
          headline="Webhook received"
          createdAt={AT}
          messageRole="system"
        >
          <span>delivery context</span>
        </FleetActivityRow>
      </SenderLabelProvider>,
    );
    const time = container.querySelector("time") as HTMLTimeElement;
    const accessibleTime = screen.getByText(/^Occurred /, {
      selector: ".sr-only",
    });
    const accessibleText = accessibleTime.textContent;
    expect(time.textContent).toBe("2 minutes ago");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });

    expect(time.textContent).toBe("3 minutes ago");
    expect(accessibleTime.textContent).toBe(accessibleText);
  });
});

describe("FleetGroupRow", () => {
  function renderGroup(expanded: boolean, outcome?: string, failed = false) {
    const onToggle = vi.fn();
    const view = render(
      <FleetGroupRow
        sender="GitHub App"
        headline="Webhook received"
        outcome={outcome}
        failed={failed}
        count={2}
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
    const times = container.querySelectorAll("time");
    expect(container.textContent).not.toContain("No outcome");
    expect(times).toHaveLength(1);
    expect(times[0]?.textContent).toMatch(/ago$/);
    expect(times[0]?.title).not.toBe("");
    fireEvent.click(screen.getByRole("button", { name: /Webhook received/ }));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("toggles an expanded group closed", () => {
    const { onToggle } = renderGroup(true, "No outcome");
    fireEvent.click(screen.getByRole("button", { name: /Webhook received/ }));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("pairs a destructive repeat count with a neutral failure result", () => {
    renderGroup(false, "Failed a startup safety check", true);

    const count = screen.getByTestId("group-count");
    const outcome = screen.getByText("Failed a startup safety check");
    expect(count.className).toMatch(/text-destructive/);
    expect(outcome.className).toMatch(/text-foreground/);
  });
});

describe("SenderLabelProvider", () => {
  function Probe() {
    return <span>{useSenderLabel() || "(none)"}</span>;
  }

  it("carries the console's fleet name to rows the thread primitive renders", () => {
    render(
      <SenderLabelProvider senderLabel="github-pr-reviewer">
        <Probe />
      </SenderLabelProvider>,
    );
    expect(screen.getByText("github-pr-reviewer")).toBeTruthy();
  });

  it("reads as absent outside a provider rather than throwing", () => {
    render(<Probe />);
    expect(screen.getByText("(none)")).toBeTruthy();
  });
});
