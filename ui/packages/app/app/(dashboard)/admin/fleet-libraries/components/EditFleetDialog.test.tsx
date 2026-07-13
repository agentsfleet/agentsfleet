import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { PlatformCatalogEntry } from "@/lib/types";
import EditFleetDialog from "./EditFleetDialog";

// The pencil. It writes the only two fields no bundle can supply — and the server
// keeps both out of the refetch upsert precisely so an edit here survives the next
// `Fetch update`. If this dialog wrote anything the bundle also owns, a refetch
// would silently undo the operator's work.
const patchPlatformLibraryActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  patchPlatformLibraryAction: (...args: unknown[]) => patchPlatformLibraryActionMock(...args),
}));

const ENTRY: PlatformCatalogEntry = {
  id: "github-pr-reviewer",
  name: "GitHub Pull Request reviewer",
  description: "Reviews pull requests.",
  source_repo: "agentsfleet/github-pr-reviewer",
  source_ref: "main",
  visibility: "draft",
  content_hash: "abc123",
  requirements: {
    credentials: ["github"],
    tools: ["http_request"],
    network_hosts: ["api.github.com"],
    trigger_present: true,
  },
  required_credentials_reasons: { github: "review your pull requests" },
  support_files: [],
  updated_at: 1_700_000_000_000,
};

function renderDialog(entry: PlatformCatalogEntry = ENTRY, onOpenChange = vi.fn()) {
  render(
    <TooltipProvider>
      <EditFleetDialog entry={entry} open onOpenChange={onOpenChange} />
    </TooltipProvider>,
  );
  return onOpenChange;
}

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(cleanup);

describe("EditFleetDialog", () => {
  it("opens with the entry's current copy, not an empty form", () => {
    renderDialog();

    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe(
      "Reviews pull requests.",
    );
    expect((screen.getByLabelText("github") as HTMLInputElement).value).toBe(
      "review your pull requests",
    );
  });

  // An operator can only explain the credentials the BUNDLE declares — they cannot
  // invent one the fleet never asks for, because the install gate would never show it.
  it("offers one reason field per credential the bundle declares", () => {
    renderDialog({
      ...ENTRY,
      requirements: { ...ENTRY.requirements, credentials: ["github", "slack"] },
    });

    expect(screen.getByLabelText("github")).toBeTruthy();
    expect(screen.getByLabelText("slack")).toBeTruthy();
    expect(screen.queryByLabelText("zoho")).toBeNull();
  });

  it("saves the description and the per-credential copy, and closes", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    const onOpenChange = renderDialog();

    const description = screen.getByLabelText("Description");
    await user.clear(description);
    await user.type(description, "Reviews your pull requests and comments.");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("github-pr-reviewer", {
        description: "Reviews your pull requests and comments.",
        required_credentials_reasons: { github: "review your pull requests" },
      }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("keeps the dialog open and shows the mapped error when the save fails", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });
    const onOpenChange = renderDialog();

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    expect(await screen.findByText("UZ-AUTH-022")).toBeTruthy();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(screen.getByLabelText("Description")).toBeTruthy();
  });

  it("renders no reason fields when the bundle declares no credentials", () => {
    renderDialog({
      ...ENTRY,
      requirements: { ...ENTRY.requirements, credentials: [] },
    });

    expect(screen.queryByLabelText("github")).toBeNull();
    expect(screen.getByLabelText("Description")).toBeTruthy();
  });

  // A row that has never been curated arrives with no reasons object at all.
  it("survives an entry with no curated copy yet", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    renderDialog({ ...ENTRY, required_credentials_reasons: undefined });

    await user.type(screen.getByLabelText("github"), "review your pull requests");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("github-pr-reviewer", {
        description: "Reviews pull requests.",
        required_credentials_reasons: { github: "review your pull requests" },
      }),
    );
  });

  it("Cancel closes without writing anything", async () => {
    const user = userEvent.setup();
    const onOpenChange = renderDialog();

    await user.click(screen.getByRole("button", { name: /^cancel$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(patchPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // presentError falls back to an action-derived title when the failure carries no
  // UZ code — the alert must still render, with no empty code element beside it.
  it("renders a failure that carries no code without crashing", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "network unreachable",
    });
    renderDialog();

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByLabelText("Description")).toBeTruthy();
  });
});
