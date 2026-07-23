import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import TriggerPanel, { triggerKey } from "./TriggerPanel";
import type { FleetTrigger } from "@/lib/types";

afterEach(() => cleanup());

const triggers: FleetTrigger[] = [
  { type: "webhook", source: "github", events: ["pull_request"] },
  { type: "cron", schedule: "*/15 * * * *" },
  { type: "api" },
];
const RECENT_DELIVERY_AGE_MS = 1_000;

describe("TriggerPanel", () => {
  it("renders an empty state without a webhook URL", () => {
    render(<TriggerPanel />);
    expect(screen.getByText("No triggers declared")).toBeTruthy();
    expect(screen.getByText(/saved changes take effect on the next wake/i)).toBeTruthy();
    expect(screen.queryByText(/reinstall/i)).toBeNull();
    expect(screen.queryByText(/webhook url/i)).toBeNull();
    expect(screen.queryByRole("button", { name: /copy/i })).toBeNull();
  });

  it("shows generic configured-trigger details in declared order", () => {
    render(<TriggerPanel triggers={triggers} />);
    expect(screen.getByText("github · pull_request")).toBeTruthy();
    expect(screen.getByText("*/15 * * * *")).toBeTruthy();
    expect(screen.getByText("Accepts events through the fleet API.")).toBeTruthy();
    expect(screen.queryByText(/https?:\/\//i)).toBeNull();
  });

  it("shows never-delivered state from durable delivery lookup", () => {
    render(
      <TriggerPanel
        triggers={[triggers[0]!]}
        lastDeliveryByKey={{ [triggerKey(triggers[0]!)]: null }}
      />,
    );
    expect(screen.getByText("No deliveries yet")).toBeTruthy();
  });

  it("handles a webhook without event filters and shows a durable delivery time", () => {
    const trigger: FleetTrigger = { type: "webhook", source: "zoho_desk" };
    render(
      <TriggerPanel
        triggers={[trigger]}
        lastDeliveryByKey={{ [triggerKey(trigger)]: Date.now() - RECENT_DELIVERY_AGE_MS }}
      />,
    );
    expect(screen.getByText("zoho_desk")).toBeTruthy();
    expect(screen.getByText(/Last delivery/)).toBeTruthy();
  });

  it("keeps trigger keys stable across supported trigger types", () => {
    expect(triggers.map(triggerKey)).toEqual([
      "webhook:github",
      "cron:*/15 * * * *",
      "api",
    ]);
  });
});
