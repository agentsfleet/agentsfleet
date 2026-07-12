import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import OnboardPlatformLibraryDialog from "./OnboardPlatformLibraryDialog";

// Real design-system primitives render Radix Tooltips, so a TooltipProvider
// ancestor is mandatory — the dashboard layout mounts one in production.
const onboardPlatformLibraryActionMock = vi.fn();
const captureProductEventMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
}));

const REPO = "agentsfleet/platform-ops";

const ENTRY = {
  id: "platform-ops",
  name: "Platform operations diagnostician",
  visibility: "platform" as const,
  content_hash: "sha256:abc123",
  requirements: { credentials: ["fly"], tools: ["http_request"], network_hosts: [], trigger_present: true },
  support_files: [],
};

function renderDialog(onOnboarded = vi.fn()) {
  render(
    <TooltipProvider>
      <OnboardPlatformLibraryDialog onOnboarded={onOnboarded} />
    </TooltipProvider>,
  );
  return onOnboarded;
}

async function openAndSubmit(user: ReturnType<typeof userEvent.setup>, repo: string) {
  await user.click(screen.getByRole("button", { name: /onboard fleet/i }));
  const input = await screen.findByLabelText(/repository/i);
  if (repo) await user.type(input, repo);
  await user.click(screen.getByRole("button", { name: /^onboard$/i }));
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("OnboardPlatformLibraryDialog", () => {
  it("rejects a source_ref that is not owner/repo, without calling the action", async () => {
    const user = userEvent.setup();
    renderDialog();
    await openAndSubmit(user, "notarepo");

    expect(await screen.findByText(/use owner\/repo/i)).toBeTruthy();
    expect(onboardPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  it("rejects an empty repository, without calling the action", async () => {
    const user = userEvent.setup();
    renderDialog();
    await openAndSubmit(user, "");

    await waitFor(() => expect(onboardPlatformLibraryActionMock).not.toHaveBeenCalled());
  });

  it("onboards the repository and hands the entry back to the page", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    const onOnboarded = renderDialog();

    await openAndSubmit(user, REPO);

    await waitFor(() => expect(onOnboarded).toHaveBeenCalledWith(ENTRY));
    expect(onboardPlatformLibraryActionMock).toHaveBeenCalledWith({
      source_kind: "github",
      source_ref: REPO,
    });
  });

  it("emits the onboarding event with the catalog id and no repository free-text", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    renderDialog();

    await openAndSubmit(user, REPO);

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [event, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(event).toBe(EVENTS.platform_library_onboarded);
    expect(props).toEqual({ source_kind: "github", outcome: "success", entry_id: "platform-ops" });
  });

  it("keeps the dialog open and shows the mapped error when the backend refuses", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });
    const onOnboarded = renderDialog();

    await openAndSubmit(user, REPO);

    expect(await screen.findByText("UZ-AUTH-022")).toBeTruthy();
    expect(onOnboarded).not.toHaveBeenCalled();
    // The repository field is still mounted — the operator can correct and retry.
    expect(screen.getByLabelText(/repository/i)).toBeTruthy();
  });

  it("records a failed onboard as an outcome rather than dropping the signal", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "no SKILL.md at the repository root",
      errorCode: "UZ-BUNDLE-002",
    });
    renderDialog();

    await openAndSubmit(user, REPO);

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(props).toEqual({ source_kind: "github", outcome: "failure" });
  });
});
