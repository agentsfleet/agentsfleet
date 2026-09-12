import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import type { PlatformCatalogEntry } from "@/lib/types";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "@/components/domain/fleet-library/bundle-files";
import AddFleetDialog from "./AddFleetDialog";

/*
 * A timeout is our patience running out, not the daemon stopping.
 *
 * The acceptance trace that started this recorded
 * `POST /admin/fleet-libraries 200 time=10102.9ms` — the server answered, and
 * the client had already hung up. Raising the budget to 30s makes that rarer;
 * it cannot make it impossible, because no budget can. What these tests pin is
 * the behaviour on the other side of the budget: before reporting a failure the
 * dialog asks the catalog, and only the catalog decides.
 */

const onboardPlatformLibraryActionMock = vi.fn();
const readPlatformLibraryActionMock = vi.fn();
const captureProductEventMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
  readPlatformLibraryAction: (...args: unknown[]) => readPlatformLibraryActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
}));

const REPO = "agentsfleet/platform-ops";
const TIMEOUT = { ok: false, error: "the backend took too long to answer", errorCode: "TIMEOUT" };

function entry(contentHash: string | null): PlatformCatalogEntry {
  return {
    id: "platform-ops",
    name: "Platform operations diagnostician",
    description: "",
    source_repo: REPO,
    source_ref: "main",
    visibility: "public",
    content_hash: contentHash,
    requirements: { credentials: [] } as unknown as PlatformCatalogEntry["requirements"],
    etag: 'W/"1"',
    updated_at: 1,
  };
}

function Harness({
  entries,
  prefillRepo,
}: {
  entries: readonly PlatformCatalogEntry[];
  prefillRepo?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <TooltipProvider>
      <Button type="button" onClick={() => setOpen(true)}>
        open
      </Button>
      <AddFleetDialog
        open={open}
        onOpenChange={setOpen}
        entries={entries}
        prefillRepo={prefillRepo}
      />
    </TooltipProvider>
  );
}

async function submitAgainst(
  entries: readonly PlatformCatalogEntry[],
  { prefillRepo }: { prefillRepo?: string } = {},
) {
  const user = userEvent.setup();
  render(<Harness entries={entries} prefillRepo={prefillRepo} />);
  await user.click(screen.getByRole("button", { name: /^open$/i }));
  const input = await screen.findByLabelText(/repository/i);
  if (!prefillRepo) await user.type(input, REPO);
  await user.click(screen.getByRole("button", { name: prefillRepo ? /fetch/i : "Create" }));
  return user;
}

function outcomes() {
  return captureProductEventMock.mock.calls
    .filter(([event]) => event === EVENTS.platform_library_onboarded)
    .map(([, props]) => (props as { outcome: string }).outcome);
}

beforeEach(() => {
  vi.clearAllMocks();
  onboardPlatformLibraryActionMock.mockResolvedValue(TIMEOUT);
});

afterEach(() => {
  cleanup();
});

describe("AddFleetDialog — a timeout is settled by the catalog", () => {
  it("should close and report success when the import landed after we stopped waiting", async () => {
    // The bug this pins: the operator was told the import failed while the row
    // it created was already in the catalog they were looking at.
    readPlatformLibraryActionMock.mockResolvedValue({
      ok: true,
      data: { entries: [entry("sha256:abc123")] },
    });

    await submitAgainst([]);

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual(["success"]);
    expect(screen.queryByText("TIMEOUT")).toBeNull();
  });

  it("should keep the timeout when the catalog still has no such row", async () => {
    readPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: { entries: [] } });

    await submitAgainst([]);

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should keep the timeout when the new row carries no bundle", async () => {
    // A row with a null content_hash is an import that never reached object
    // storage. Calling that success points the operator at an unpublishable row.
    readPlatformLibraryActionMock.mockResolvedValue({
      ok: true,
      data: { entries: [entry(null)] },
    });

    await submitAgainst([]);

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should keep the timeout when a refetch left the bundle unchanged", async () => {
    // The refetch path's trap: the row was already there with a hash, so its
    // presence proves nothing. Only a CHANGED hash proves the fetch landed.
    const before = [entry("sha256:abc123")];
    readPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: { entries: before } });

    await submitAgainst(before, { prefillRepo: REPO });

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should close and report success when a refetch replaced the bundle", async () => {
    readPlatformLibraryActionMock.mockResolvedValue({
      ok: true,
      data: { entries: [entry("sha256:def456")] },
    });

    await submitAgainst([entry("sha256:abc123")], { prefillRepo: REPO });

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual(["success"]);
  });

  it("should keep the timeout when the catalog re-read itself fails", async () => {
    // An unanswered question is not a yes.
    readPlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });

    await submitAgainst([]);

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should not re-read the catalog for an error that is already a definite answer", async () => {
    // A 403 is the backend's verdict, not our impatience. Asking again buys
    // nothing and spends a round-trip on every refusal.
    onboardPlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });

    await submitAgainst([]);

    expect(await screen.findByText("UZ-AUTH-022")).toBeTruthy();
    expect(readPlatformLibraryActionMock).not.toHaveBeenCalled();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should not ask the catalog about an upload, which has no repository to match", async () => {
    // An upload stores no source_repo, so there is no row to identify it by.
    // The timeout stands as reported rather than being guessed at.
    const user = userEvent.setup();
    render(<Harness entries={[]} />);
    await user.click(screen.getByRole("button", { name: /^open$/i }));
    await user.click(await screen.findByRole("tab", { name: "Upload from computer" }));
    await user.type(await screen.findByLabelText(SKILL_FILE_NAME), "---\nname: incident-responder\n---\nBody.");
    await user.type(screen.getByLabelText(TRIGGER_FILE_NAME), "---\nname: incident-responder\nx-agentsfleet:\n---");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(readPlatformLibraryActionMock).not.toHaveBeenCalled();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should land nothing when the operator closed the dialog while we were asking", async () => {
    // The re-read is another await, so the same staleness guard that protects
    // the submit has to protect what comes back from it: a dialog the operator
    // walked away from must not reopen as a success.
    let release!: (v: unknown) => void;
    readPlatformLibraryActionMock.mockReturnValue(new Promise((r) => { release = r; }));

    const user = await submitAgainst([]);
    await waitFor(() => expect(readPlatformLibraryActionMock).toHaveBeenCalled());
    await user.keyboard("{Escape}");

    release({ ok: true, data: { entries: [entry("sha256:abc123")] } });
    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual([]);
  });

  it("should not report a failure into a dialog the operator already closed", async () => {
    // The mirror of the case above, and the one that is only reachable through
    // the re-read: a non-timeout error has no await between the staleness check
    // and the report, so only an import we asked the catalog about can come
    // back to a dialog that is no longer there. Answering "not landed" into it
    // would raise an error on a screen the operator has moved on from.
    let release!: (v: unknown) => void;
    readPlatformLibraryActionMock.mockReturnValue(new Promise((r) => { release = r; }));

    const user = await submitAgainst([]);
    await waitFor(() => expect(readPlatformLibraryActionMock).toHaveBeenCalled());
    await user.keyboard("{Escape}");

    release({ ok: true, data: { entries: [] } });
    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(screen.queryByText("TIMEOUT")).toBeNull();
    expect(outcomes()).toEqual([]);
  });
});
