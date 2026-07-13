import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { OnboardedPlatformLibraryEntry } from "@/lib/types";
import FleetLibrariesView from "./FleetLibrariesView";

// The view has no catalog read — the platform tier has no list route — so it
// renders exactly two states: nothing onboarded yet, and the entry the server
// actually returned. These tests pin that it never invents a third.
const onboardPlatformLibraryActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: vi.fn() }));

const ENTRY: OnboardedPlatformLibraryEntry = {
  id: "platform-ops",
  name: "Platform operations diagnostician",
  visibility: "platform",
  content_hash: "sha256:abc123",
  requirements: {
    credentials: ["fly", "slack"],
    tools: ["http_request"],
    network_hosts: ["api.machines.dev"],
    trigger_present: true,
  },
  support_files: [{ path: "README.md", size_bytes: 120 }],
};

function renderView() {
  render(
    <TooltipProvider>
      <FleetLibrariesView />
    </TooltipProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("FleetLibrariesView", () => {
  it("starts with no entry and an onboard affordance, claiming nothing about the catalog", () => {
    renderView();

    expect(screen.getByRole("heading", { name: /fleet libraries/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /onboard fleet/i })).toBeTruthy();
    expect(screen.getByTestId("empty-state")).toBeTruthy();
    // No card is rendered before an onboard actually returns one.
    expect(screen.queryByTestId(`onboarded-entry-${ENTRY.id}`)).toBeNull();
  });

  it("renders the entry the server returned — catalog id, tier, and content hash", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    renderView();

    await user.click(screen.getByRole("button", { name: /onboard fleet/i }));
    await user.type(await screen.findByLabelText(/repository/i), "agentsfleet/platform-ops");
    await user.click(screen.getByRole("button", { name: /^onboard$/i }));

    const card = await screen.findByTestId(`onboarded-entry-${ENTRY.id}`);
    // The id is the bundle's declared name, not the repository path typed above.
    expect(card.textContent).toContain("platform-ops");
    expect(card.textContent).toContain("platform");
    expect(card.textContent).toContain("sha256:abc123");
    expect(card.textContent).toContain("fly, slack");
    expect(card.textContent).toContain("http_request");

    // The empty state is gone once a real entry exists.
    await waitFor(() => expect(screen.queryByTestId("empty-state")).toBeNull());
  });

  it("shows 'none' rather than an empty gap for a bundle that declares no requirements", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        ...ENTRY,
        requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: false },
        support_files: [],
      },
    });
    renderView();

    await user.click(screen.getByRole("button", { name: /onboard fleet/i }));
    await user.type(await screen.findByLabelText(/repository/i), "agentsfleet/platform-ops");
    await user.click(screen.getByRole("button", { name: /^onboard$/i }));

    const card = await screen.findByTestId(`onboarded-entry-${ENTRY.id}`);
    expect(card.textContent).toContain("none");
  });
});
