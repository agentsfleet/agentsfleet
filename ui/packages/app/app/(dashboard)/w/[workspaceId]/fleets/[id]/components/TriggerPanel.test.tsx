import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import TriggerPanel, { triggerKey } from "./TriggerPanel";
import type { FleetTrigger } from "@/lib/types";

afterEach(() => cleanup());

const triggers: FleetTrigger[] = [
  { type: "webhook", source: "github", events: ["pull_request"] },
  { type: "cron", schedule: "*/15 * * * *" },
  { type: "mention", source: "slack", channels: ["C0123456789"] },
];
const RECENT_DELIVERY_AGE_MS = 1_000;

describe("TriggerPanel", () => {
  it.each([{triggers: undefined}, {triggers: []}])("explains chat and steer access with no external triggers (%j)", ({triggers: empty}) => {
    render(<TriggerPanel triggers={empty} />);
    expect(screen.getByText("Runs when you send a message")).toBeTruthy();
    expect(screen.getByText(/No automatic triggers or schedules/)).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.queryByText(/reinstall/i)).toBeNull();
    expect(screen.queryByText(/webhook url/i)).toBeNull();
    expect(screen.queryByRole("button", { name: /copy/i })).toBeNull();
  });

  it("shows generic configured-trigger details in declared order", () => {
    render(<TriggerPanel triggers={triggers} />);
    expect(screen.getByText(/When github sends pull_request/)).toBeTruthy();
    expect(screen.getByText(/On a schedule:/)).toBeTruthy();
    expect(screen.getByText("Chat mention")).toBeTruthy();
    expect(screen.getByText(/When mentioned in slack: C0123456789/)).toBeTruthy();
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
    const trigger: FleetTrigger = { type: "webhook", source: "linear" };
    render(
      <TriggerPanel
        triggers={[trigger]}
        lastDeliveryByKey={{ [triggerKey(trigger)]: new Date(Date.now() - RECENT_DELIVERY_AGE_MS).toISOString() }}
      />,
    );
    expect(screen.getByText(/When linear sends an event/)).toBeTruthy();
    expect(screen.getByText(/Last delivery/)).toBeTruthy();
  });

  it("keeps failed history lookups distinct from no deliveries", () => {
    render(<TriggerPanel triggers={[triggers[0]!]} lastDeliveryByKey={{}} />);
    expect(screen.getByText("Delivery history unavailable")).toBeTruthy();
    expect(screen.queryByText("No deliveries yet")).toBeNull();
  });

  it("keeps trigger keys stable across supported trigger types", () => {
    expect(triggers.map(triggerKey)).toEqual([
      "webhook:github",
      "cron:*/15 * * * *",
      "mention:slack:C0123456789",
    ]);
  });
});
