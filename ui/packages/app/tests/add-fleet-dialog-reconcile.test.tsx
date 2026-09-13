import React from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { afterEach, describe, expect, it, vi } from "vitest";

import AddFleetDialog from "@/app/(dashboard)/admin/fleet-libraries/components/AddFleetDialog";
import { RECONCILE_ATTEMPTS } from "@/app/(dashboard)/admin/fleet-libraries/import-reconcile";
import { RETRY_CODE_TIMEOUT } from "@/lib/api/errors";
import type { PlatformCatalogEntry } from "@/lib/types";

const onboardPlatformLibraryAction = vi.fn();
const readPlatformLibraryAction = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryAction(...args),
  readPlatformLibraryAction: (...args: unknown[]) => readPlatformLibraryAction(...args),
}));

const captureProductEvent = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEvent(...args),
}));

const REPO = "agentsfleet/github-pr-reviewer";
const STAMP = 1_700_000_000_000;

function entry(overrides: Partial<PlatformCatalogEntry> = {}): PlatformCatalogEntry {
  return {
    id: "github-pr-reviewer",
    name: "GitHub PR reviewer",
    description: "Reviews pull requests.",
    source_repo: REPO,
    source_ref: "main",
    visibility: "draft",
    content_hash: "sha256:abc",
    requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
    etag: "etag-1",
    updated_at: STAMP,
    ...overrides,
  };
}

function renderDialog(onOpenChange: (open: boolean) => void) {
  return render(
    <TooltipProvider>
      <AddFleetDialog
        open
        onOpenChange={onOpenChange}
        prefillRepo={REPO}
        prefillRef="main"
        entries={[entry()]}
      />
    </TooltipProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AddFleetDialog — a response the operator walked away from", () => {
  it("drops a reconcile that finishes after the dialog was closed", async () => {
    // The reconcile polls for several seconds after a timeout. That is a real
    // window for the operator to hit Cancel, and a verdict arriving afterwards
    // must not report an outcome or reopen anything. `requestIdRef` is what
    // makes the late answer inert; this is the case that proves it.
    let releaseRead: (value: unknown) => void = () => {};
    const readInFlight = new Promise((resolve) => {
      releaseRead = resolve;
    });

    onboardPlatformLibraryAction.mockResolvedValue({
      ok: false,
      errorCode: RETRY_CODE_TIMEOUT,
      error: "timed out",
    });
    readPlatformLibraryAction.mockImplementation(async () => {
      await readInFlight;
      // The import did land: same bundle, newer stamp.
      return { ok: true, data: { entries: [entry({ updated_at: STAMP + 1 })] } };
    });

    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    fireEvent.submit(screen.getByRole("button", { name: /fetch update/i }).closest("form")!);
    await waitFor(() => expect(readPlatformLibraryAction).toHaveBeenCalled());

    // The operator gives up while the catalog is still being asked. Cancel and
    // Submit are both disabled while pending, so Escape is the close path that
    // is actually open to them here — Radix routes it to the same handler.
    fireEvent.keyDown(document.body, { key: "Escape", code: "Escape" });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    onOpenChange.mockClear();
    captureProductEvent.mockClear();

    releaseRead(null);
    await waitFor(() => expect(readPlatformLibraryAction).toHaveBeenCalled());

    // The abandoned submit reports nothing and closes nothing a second time.
    expect(captureProductEvent).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("drops a reconcile that came back empty-handed after the dialog was closed", async () => {
    // The other side of the abandoned window: the poll runs its attempts and
    // concludes the import never landed. That verdict is just as stale as a
    // success, so it must not raise an error alert on a dialog the operator
    // has already dismissed.
    let releaseRead: (value: unknown) => void = () => {};
    const readInFlight = new Promise((resolve) => {
      releaseRead = resolve;
    });

    onboardPlatformLibraryAction.mockResolvedValue({
      ok: false,
      errorCode: RETRY_CODE_TIMEOUT,
      error: "timed out",
    });
    // Every attempt sees the row exactly as it was: nothing landed.
    readPlatformLibraryAction.mockImplementation(async () => {
      await readInFlight;
      return { ok: true, data: { entries: [entry()] } };
    });

    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    fireEvent.submit(screen.getByRole("button", { name: /fetch update/i }).closest("form")!);
    await waitFor(() => expect(readPlatformLibraryAction).toHaveBeenCalled());

    fireEvent.keyDown(document.body, { key: "Escape", code: "Escape" });
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    captureProductEvent.mockClear();

    releaseRead(null);
    await waitFor(() =>
      expect(readPlatformLibraryAction.mock.calls.length).toBe(RECONCILE_ATTEMPTS),
    );

    // No failure event, and no error alert on a dialog nobody is looking at.
    expect(captureProductEvent).not.toHaveBeenCalled();
    expect(screen.queryByText(/timed out/i)).toBeNull();
  }, 15_000);

  it("reports success when the reconcile finds the import landed on a later attempt", async () => {
    onboardPlatformLibraryAction.mockResolvedValue({
      ok: false,
      errorCode: RETRY_CODE_TIMEOUT,
      error: "timed out",
    });
    // The first read fires before the upsert; the second sees the newer stamp.
    readPlatformLibraryAction
      .mockResolvedValueOnce({ ok: true, data: { entries: [entry()] } })
      .mockResolvedValue({ ok: true, data: { entries: [entry({ updated_at: STAMP + 1 })] } });

    const onOpenChange = vi.fn();
    renderDialog(onOpenChange);

    fireEvent.submit(screen.getByRole("button", { name: /fetch update/i }).closest("form")!);

    await waitFor(
      () =>
        expect(captureProductEvent).toHaveBeenCalledWith(
          expect.anything(),
          expect.objectContaining({ outcome: "success" }),
        ),
      { timeout: 10_000 },
    );
    expect(readPlatformLibraryAction.mock.calls.length).toBeLessThanOrEqual(RECONCILE_ATTEMPTS);
  }, 15_000);
});
