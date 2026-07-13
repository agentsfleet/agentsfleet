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

  it("Cancel closes the dialog and clears what was typed, so a reopen starts clean", async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.click(screen.getByRole("button", { name: /onboard fleet/i }));
    await user.type(await screen.findByLabelText(/repository/i), REPO);
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(onboardPlatformLibraryActionMock).not.toHaveBeenCalled();

    // Reopening must not resurrect the abandoned repository.
    await user.click(screen.getByRole("button", { name: /onboard fleet/i }));
    expect((await screen.findByLabelText(/repository/i)).getAttribute("value")).toBe("");
  });

  it("disables the controls and shows a spinner while the onboard is in flight", async () => {
    const user = userEvent.setup();
    let release: (v: unknown) => void = () => {};
    onboardPlatformLibraryActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        release = resolve;
      }),
    );
    renderDialog();

    await openAndSubmit(user, REPO);

    // A slow importer (GitHub fetch + validate + object-store write) must not
    // let the operator fire a second onboard on top of the first.
    const submit = await screen.findByRole("button", { name: /onboarding fleet|onboard$/i });
    await waitFor(() => expect(submit.hasAttribute("disabled")).toBe(true));
    expect(screen.getByRole("button", { name: /^cancel$/i }).hasAttribute("disabled")).toBe(true);

    release({ ok: true, data: ENTRY });
    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
  });

  it("drops a response the operator already abandoned by closing the dialog", async () => {
    const user = userEvent.setup();
    let release: (v: unknown) => void = () => {};
    onboardPlatformLibraryActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        release = resolve;
      }),
    );
    const onOnboarded = renderDialog();

    await openAndSubmit(user, REPO);
    // Dismiss while the onboard is still in flight. Cancel is disabled during a
    // submit, so Escape (Radix's onOpenChange, which `pending` does not gate) is
    // the real abandon path — and it is exactly what the requestId guard exists
    // for. The importer keeps running server-side; its answer is no longer wanted.
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());

    release({ ok: true, data: ENTRY });

    // The stale success must not reopen the page's result card, and it must not
    // resurrect the dialog. Without the requestId guard this would fire.
    await waitFor(() => expect(onOnboarded).not.toHaveBeenCalled());
    expect(screen.queryByLabelText(/repository/i)).toBeNull();
  });

  it("renders a mapped error that carries no body or code without crashing", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "network unreachable",
    });
    renderDialog();

    await openAndSubmit(user, REPO);

    // presentError falls back to an action-derived title when the failure has
    // no UZ code — the alert must still render, with no empty code element.
    expect(await screen.findByRole("alert")).toBeTruthy();
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
