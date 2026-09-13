import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import type { PlatformCatalogEntry } from "@/lib/types";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "@/components/domain/fleet-library/bundle-files";
import AddFleetDialog from "./AddFleetDialog";
import { RECONCILE_ATTEMPTS } from "../import-reconcile";

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

const STAMP = 1_700_000_000_000;

function entry(contentHash: string | null, updatedAt: number = STAMP): PlatformCatalogEntry {
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
    updated_at: updatedAt,
  };
}

/** The catalog answers in order: read one is the baseline, the rest are the poll. */
function catalogReads(
  baseline: PlatformCatalogEntry[] | null,
  ...polls: (PlatformCatalogEntry[] | null)[]
) {
  const answer = (rows: PlatformCatalogEntry[] | null) =>
    rows === null
      ? { ok: false, error: "insufficient scope", errorCode: "UZ-AUTH-022" }
      : { ok: true, data: { entries: rows } };
  readPlatformLibraryActionMock.mockResolvedValueOnce(answer(baseline));
  for (const rows of polls.slice(0, -1)) {
    readPlatformLibraryActionMock.mockResolvedValueOnce(answer(rows));
  }
  const last = polls.at(-1);
  if (last) readPlatformLibraryActionMock.mockResolvedValue(answer(last));
}

function Harness({ prefillRepo }: { prefillRepo?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <TooltipProvider>
      <Button type="button" onClick={() => setOpen(true)}>
        open
      </Button>
      <AddFleetDialog open={open} onOpenChange={setOpen} prefillRepo={prefillRepo} />
    </TooltipProvider>
  );
}

/*
 * The dialog reads its own baseline at submit time, so a test states the
 * catalog as a SEQUENCE: the first read is the before state, every read after
 * it is the reconcile poll. `catalogReads` spells that out per case.
 */
async function submitAgainst(
  { prefillRepo }: { prefillRepo?: string } = {},
) {
  const user = userEvent.setup();
  render(<Harness prefillRepo={prefillRepo} />);
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
    catalogReads([], [entry("sha256:abc123")]);

    await submitAgainst();

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual(["success"]);
    expect(screen.queryByText("TIMEOUT")).toBeNull();
  });

  it("should keep the timeout when the catalog still has no such row", async () => {
    catalogReads([], []);

    await submitAgainst();

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should keep the timeout when the new row carries no bundle", async () => {
    // A row with a null content_hash is an import that never reached object
    // storage. Calling that success points the operator at an unpublishable row.
    catalogReads([], [entry(null)]);

    await submitAgainst();

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should keep the timeout when a refetch changed neither the bundle nor the stamp", async () => {
    // The refetch path's trap: the row was already there with a hash, so its
    // presence proves nothing. Nothing was written at all here.
    catalogReads([entry("sha256:abc123")], [entry("sha256:abc123")]);

    await submitAgainst({ prefillRepo: REPO });

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should close and report success when a refetch of an unmoved branch bumped the stamp", async () => {
    // A branch that has not moved yields the same bundle, so the hash alone
    // would call a completed refetch a failure. core.fleet_library's upsert
    // writes updated_at with no equality guard, so a landed import moves it.
    catalogReads([entry("sha256:abc123", STAMP)], [entry("sha256:abc123", STAMP + 1)]);

    await submitAgainst({ prefillRepo: REPO });

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual(["success"]);
  });

  it("should not read someone else's write as this refetch landing", async () => {
    // The baseline is read at submit time, not taken from the page's render.
    // Another tab, another operator, or an earlier timed-out import can advance
    // this row between the page drawing and this submit; a baseline captured at
    // render would see that stamp move and close the dialog on a refetch that
    // actually failed. Reading now means the foreign write is already IN the
    // baseline, so only a further advance counts.
    const alreadyBumped = entry("sha256:abc123", STAMP + 500);
    catalogReads([alreadyBumped], [alreadyBumped]);

    await submitAgainst({ prefillRepo: REPO });

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should close and report success when a refetch replaced the bundle", async () => {
    catalogReads([entry("sha256:abc123")], [entry("sha256:def456")]);

    await submitAgainst({ prefillRepo: REPO });

    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual(["success"]);
  });

  it("should keep the timeout when the baseline read itself fails", async () => {
    // With no trustworthy before state there is nothing to compare against, and
    // a guess is worse than the honest answer. An unanswered question is not a
    // yes — and here it is not even a question we got to ask.
    catalogReads(null, []);

    await submitAgainst();

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
    // The baseline failed, so the poll never ran.
    expect(readPlatformLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("should keep the timeout when the catalog re-read fails after a good baseline", async () => {
    catalogReads([], null);

    await submitAgainst();

    expect(await screen.findByText("TIMEOUT")).toBeTruthy();
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should not re-read the catalog for an error that is already a definite answer", async () => {
    // A 403 is the backend's verdict, not our impatience. The baseline read has
    // already happened by then — it has to, to be of any use — but the poll
    // that would spend four more round-trips does not run.
    catalogReads([], []);
    onboardPlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });

    await submitAgainst();

    expect(await screen.findByText("UZ-AUTH-022")).toBeTruthy();
    expect(readPlatformLibraryActionMock).toHaveBeenCalledTimes(1);
    expect(outcomes()).toEqual(["failure"]);
  });

  it("should not ask the catalog about an upload, which has no repository to match", async () => {
    // An upload stores no source_repo, so there is no row to identify it by.
    // It takes no baseline read either — there is nothing to take one of.
    const user = userEvent.setup();
    render(<Harness />);
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
    // The poll is a run of awaits, so the staleness guard that protects the
    // submit has to protect what comes back from it: a dialog the operator
    // walked away from must not reopen as a success.
    let release!: (v: unknown) => void;
    readPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: { entries: [] } });
    readPlatformLibraryActionMock.mockReturnValue(new Promise((r) => { release = r; }));

    const user = await submitAgainst();
    await waitFor(() => expect(readPlatformLibraryActionMock).toHaveBeenCalledTimes(2));
    await user.keyboard("{Escape}");

    release({ ok: true, data: { entries: [entry("sha256:abc123")] } });
    await waitFor(() => expect(screen.queryByLabelText(/repository/i)).toBeNull());
    expect(outcomes()).toEqual([]);
  });

  it("should not report a failure into a dialog the operator already closed", async () => {
    // The mirror of the case above: answering "not landed" into a dialog the
    // operator has moved on from would raise an error on a screen nobody is
    // looking at. Only the poll can come back this late.
    readPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: { entries: [] } });
    readPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: { entries: [] } });

    const user = await submitAgainst();
    await waitFor(() => expect(readPlatformLibraryActionMock).toHaveBeenCalledTimes(2));
    await user.keyboard("{Escape}");

    // Let every attempt run out, which is when the "not landed" verdict lands.
    await waitFor(
      () =>
        expect(readPlatformLibraryActionMock).toHaveBeenCalledTimes(RECONCILE_ATTEMPTS + 1),
      { timeout: 15_000 },
    );
    expect(screen.queryByText("TIMEOUT")).toBeNull();
    expect(outcomes()).toEqual([]);
  }, 25_000);
});
