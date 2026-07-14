import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { GuidanceCard } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/provider-guidance";
import GuidedTriggerCard from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/GuidedTriggerCard";

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    CheckIcon: make("CheckIcon"),
    CopyIcon: make("CopyIcon"),
    ExternalLinkIcon: make("ExternalLinkIcon"),
  };
});

const writeText = vi.fn();

const GUIDANCE: GuidanceCard = {
  title: "GitHub",
  eventsLabel: (events) => `On ${events.join(", ")}`,
  command: (_vars, webhookUrl) => `register ${webhookUrl}`,
  webUiDeepLink: () => "https://github.com/acme/repo/settings/hooks/new",
  variables: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  writeText.mockResolvedValue(undefined);
});

afterEach(() => cleanup());

describe("GuidedTriggerCard", () => {
  it("shows copied state after copying the webhook URL", async () => {
    const user = userEvent.setup({ delay: null });
    render(
      <GuidedTriggerCard
        trigger={{ type: "webhook", source: "github", events: ["pull_request"] }}
        webhookUrl="https://agentsfleet.test/hooks/1"
        guidance={GUIDANCE}
      />,
    );

    // Capture the button BEFORE clicking. Its accessible name flips to "Copied"
    // on success — that IS the behaviour under test — so re-querying by the idle
    // name would silently find the *other* copy button on the card and assert
    // against the wrong node. React updates in place, so the ref stays valid.
    const copyUrl = screen.getAllByRole("button", { name: /copy webhook url/i })[0]!;

    await user.click(copyUrl);

    await waitFor(() => expect(copyUrl.textContent).toContain("Copied"));
  });
});
