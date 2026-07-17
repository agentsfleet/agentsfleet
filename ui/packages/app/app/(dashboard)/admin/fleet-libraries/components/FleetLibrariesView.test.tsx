import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import type { PlatformCatalogEntry } from "@/lib/types";
import FleetLibrariesView from "./FleetLibrariesView";

// The view now reads the catalog, so it renders THREE row states — published,
// draft, and a row whose bundle was never fetched — and offers only the actions
// each state can actually serve. These tests pin that it never offers one it
// cannot honour: a disabled or lying affordance is worse than none.
const onboardPlatformLibraryActionMock = vi.fn();
const patchPlatformLibraryActionMock = vi.fn();
const deletePlatformLibraryActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
  patchPlatformLibraryAction: (...args: unknown[]) => patchPlatformLibraryActionMock(...args),
  deletePlatformLibraryAction: (...args: unknown[]) => deletePlatformLibraryActionMock(...args),
}));
const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
}));

function entry(over: Partial<PlatformCatalogEntry> = {}): PlatformCatalogEntry {
  return {
    id: "platform-ops",
    name: "Platform operations diagnostician",
    description: "Diagnoses platform incidents.",
    source_repo: "agentsfleet/platform-ops",
    source_ref: "main",
    visibility: "draft",
    content_hash: "abc123def456789",
    requirements: {
      credentials: ["fly", "slack"],
      tools: ["http_request"],
      network_hosts: ["api.machines.dev"],
      trigger_present: true,
    },
    required_credentials_reasons: {},
    support_files: [{ path: "README.md", size_bytes: 120 }],
    etag: '"catalog-v1"',
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

const PUBLISHED = entry({ id: "github-pr-reviewer", name: "Reviewer", visibility: "public" });
const DRAFT = entry();
const PUBLISHED_DRAFT = entry({ visibility: "public", etag: '"catalog-v2"' });
const NO_BUNDLE = entry({ id: "zoho-sprint", name: "Zoho", content_hash: null });
const MISTYPED_REPO = "agentsfleet/mistyped";

function renderView(entries: PlatformCatalogEntry[]) {
  render(
    <TooltipProvider>
      <FleetLibrariesView entries={entries} />
    </TooltipProvider>,
  );
}

describe("FleetLibrariesView", () => {
  beforeEach(() => {
    onboardPlatformLibraryActionMock.mockReset();
    patchPlatformLibraryActionMock.mockReset();
    deletePlatformLibraryActionMock.mockReset();
    captureProductEventMock.mockReset();
  });
  afterEach(cleanup);

  it("renders one row per catalog entry, each with its status", () => {
    renderView([PUBLISHED, DRAFT, NO_BUNDLE]);

    expect(screen.getByText("Reviewer")).toBeTruthy();
    expect(screen.getByText("Platform operations diagnostician")).toBeTruthy();
    expect(screen.getByText("Zoho")).toBeTruthy();

    expect(screen.getByText("Published")).toBeTruthy();
    expect(screen.getByText("Draft")).toBeTruthy();
    expect(screen.getByText("No bundle")).toBeTruthy();
  });

  it("shows the empty state only when the catalog is genuinely empty", () => {
    renderView([]);
    expect(screen.getByText("No fleets in the catalog")).toBeTruthy();

    cleanup();
    renderView([DRAFT]);
    expect(screen.queryByText("No fleets in the catalog")).toBeNull();
  });

  it("offers Publish on a draft and Unpublish on a published fleet, never both", () => {
    renderView([DRAFT]);
    expect(screen.getByRole("button", { name: "Publish" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Unpublish" })).toBeNull();

    cleanup();
    renderView([PUBLISHED]);
    expect(screen.getByRole("button", { name: "Unpublish" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Publish" })).toBeNull();
  });

  // A row with no bundle has nothing to serve a tenant, so publishing it is a 409
  // the server would refuse — the UI must not offer the button at all.
  it("never offers Publish on a row whose bundle was never fetched", () => {
    renderView([NO_BUNDLE]);
    expect(screen.queryByRole("button", { name: "Publish" })).toBeNull();
    expect(screen.getByRole("button", { name: "Fetch bundle" })).toBeTruthy();
  });

  // Withdraw before deleting: a live fleet is never taken from the tenants who can
  // install it. A disabled button would be a promise, so there is none.
  it("offers Delete only on an unpublished fleet", () => {
    renderView([DRAFT]);
    expect(screen.getByRole("button", { name: "Delete" })).toBeTruthy();

    cleanup();
    renderView([PUBLISHED]);
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
  });

  it("publishes a draft through the patch action", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: PUBLISHED_DRAFT });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Publish" }));

    await waitFor(() => {
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("platform-ops", {
        published: true,
      }, DRAFT.etag);
    });
    expect(await screen.findByText("Published")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Unpublish" })).toHaveProperty("disabled", false);
  });

  it("surfaces a failed publish instead of silently doing nothing", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "no bundle",
      errorCode: "UZ-CATALOG-002",
    });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Publish" }));

    expect(await screen.findByTestId("catalog-error")).toBeTruthy();
  });

  // The operator never retypes a repository the table is already showing them.
  it("prefills the dialog with the row's repository when fetching an update", async () => {
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Fetch update" }));

    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: "Fetch update" })).toBeTruthy();
      const field = screen.getByLabelText("Repository") as HTMLInputElement;
      expect(field.value).toBe("agentsfleet/platform-ops");
    });
  });

  it("fetches from the repository returned by the completed edit", async () => {
    const stale = entry({ content_hash: null, source_repo: MISTYPED_REPO });
    const corrected = { ...stale, source_repo: DRAFT.source_repo, etag: '"catalog-v2"' };
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: corrected });
    renderView([stale]);

    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    const repository = screen.getByLabelText("Repository");
    await userEvent.clear(repository);
    await userEvent.type(repository, DRAFT.source_repo);
    await userEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(screen.queryByText("Edit fleet library")).toBeNull());

    await userEvent.click(screen.getByRole("button", { name: "Fetch bundle" }));
    await waitFor(() => {
      const field = screen.getByLabelText("Repository") as HTMLInputElement;
      expect(field.value).toBe(DRAFT.source_repo);
    });
  });

  it("keeps the latest server row across consecutive writes before revalidation", async () => {
    const stale = entry({ visibility: DRAFT.visibility, etag: '"catalog-v1"' });
    const edited = { ...stale, description: "Edited", etag: '"catalog-v2"' };
    const published = { ...edited, visibility: "public" as const, etag: '"catalog-v3"' };
    patchPlatformLibraryActionMock
      .mockResolvedValueOnce({ ok: true, data: edited })
      .mockResolvedValueOnce({ ok: true, data: published });
    renderView([stale]);

    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    const description = screen.getByLabelText("Description");
    await userEvent.clear(description);
    await userEvent.type(description, edited.description);
    await userEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(screen.queryByText("Edit fleet library")).toBeNull());
    await userEvent.click(screen.getByRole("button", { name: "Publish" }));

    expect(patchPlatformLibraryActionMock).toHaveBeenNthCalledWith(2, stale.id, { published: true }, edited.etag);
    expect(await screen.findByText("Published")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Unpublish" })).toHaveProperty("disabled", false);
  });

  it("opens the add dialog empty, not prefilled from an earlier row", async () => {
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Fetch update" }));
    await waitFor(() => {
      expect((screen.getByLabelText("Repository") as HTMLInputElement).value).toBe(
        "agentsfleet/platform-ops",
      );
    });
    await userEvent.keyboard("{Escape}");

    await userEvent.click(screen.getByRole("button", { name: "Create fleet library" }));
    await waitFor(() => {
      expect((screen.getByLabelText("Repository") as HTMLInputElement).value).toBe("");
    });
  });

  // Delete is destructive and irreversible, so it goes behind a confirm that names
  // the fleet — and the confirm says the thing an operator actually worries about:
  // workspaces already running it are unaffected.
  it("deletes an unpublished fleet only after a confirm that names it", async () => {
    deletePlatformLibraryActionMock.mockResolvedValue({ ok: true, data: undefined });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    const confirm = await screen.findByRole("alertdialog");
    expect(within(confirm).getByText(/Delete this fleet\?/i)).toBeTruthy();
    // The confirm names the fleet, so an operator cannot delete the wrong row.
    expect(within(confirm).getByText(/Platform operations diagnostician/)).toBeTruthy();
    expect(deletePlatformLibraryActionMock).not.toHaveBeenCalled();

    await userEvent.click(within(confirm).getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(deletePlatformLibraryActionMock).toHaveBeenCalledWith("platform-ops");
    });
  });

  it("surfaces a refused delete instead of closing as though it worked", async () => {
    deletePlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "published",
      errorCode: "UZ-CATALOG-003",
    });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog");
    await userEvent.click(within(confirm).getByRole("button", { name: "Delete" }));

    // The refusal is surfaced on the page, not swallowed into a closed dialog.
    expect(await screen.findByTestId("catalog-error")).toBeTruthy();
  });

  it("opens the edit dialog for a row and closes it again", async () => {
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(await screen.findByText("Edit fleet library")).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByText("Edit fleet library")).toBeNull());
  });

  it("unpublishes a published fleet through the patch action", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: DRAFT });
    renderView([PUBLISHED]);

    await userEvent.click(screen.getByRole("button", { name: "Unpublish" }));

    await waitFor(() => {
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("github-pr-reviewer", {
        published: false,
      }, PUBLISHED.etag);
    });
  });

  // The hash is how an operator confirms a refetch changed something, so it is
  // shown — truncated — and a row with no bundle shows a definite absence, not a
  // blank cell that reads as a rendering bug.
  it("shows a truncated hash, and an em dash when there is no bundle", () => {
    renderView([DRAFT, NO_BUNDLE]);
    expect(screen.getByText("abc123def456")).toBeTruthy();
    expect(screen.getByText("—")).toBeTruthy();
  });

  // Backing out of a destructive confirm must delete nothing. The dialog is the
  // last place an operator can change their mind.
  it("cancelling the delete confirm deletes nothing", async () => {
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog");
    await userEvent.click(within(confirm).getByRole("button", { name: /^cancel$/i }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deletePlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // Dismissing the confirm must leave the fleet alone. A destructive action that
  // fires on dismissal is the worst bug this surface could have.
  it("cancelling the confirm deletes nothing", async () => {
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog");
    await userEvent.click(within(confirm).getByRole("button", { name: /^cancel$/i }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deletePlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // Publishing is the moment a fleet becomes available to every tenant — the one
  // state change here with a decision riding on it, so it is the one event added.
  it("emits the publish event with the catalog slug and no operator free-text", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: PUBLISHED_DRAFT });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Publish" }));

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [event, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(event).toBe(EVENTS.platform_library_published);
    expect(props).toEqual({ entry_id: "platform-ops", action: "published", outcome: "success" });
  });

  it("records a refused publish as an outcome rather than dropping the signal", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({
      ok: false,
      error: "no bundle",
      errorCode: "UZ-CATALOG-002",
    });
    renderView([DRAFT]);

    await userEvent.click(screen.getByRole("button", { name: "Publish" }));

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(props).toMatchObject({ action: "published", outcome: "failure" });
  });

  it("records an unpublish as its own action", async () => {
    patchPlatformLibraryActionMock.mockResolvedValue({ ok: true, data: DRAFT });
    renderView([PUBLISHED]);

    await userEvent.click(screen.getByRole("button", { name: "Unpublish" }));

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(props).toMatchObject({ action: "unpublished", outcome: "success" });
  });

  // ── Dimension 5.1 — the repository cell links only when it can ────────────

  it("links the repository to GitHub when the source is owner/repo shaped", async () => {
    renderView([entry({ id: "linked", source_repo: "agentsfleet/platform-ops" })]);

    const link = await screen.findByRole("link", { name: /agentsfleet\/platform-ops/ });
    expect(link.getAttribute("href")).toBe("https://github.com/agentsfleet/platform-ops");
    // Never a tab-hijack: external link opens away without a window handle.
    expect(link.getAttribute("rel")).toContain("noopener");
  });

  // A template- or upload-sourced row carries a source that is not a GitHub
  // slug. Linking it would point at a repository that does not exist — inert
  // text is the honest rendering.
  it("renders a non-slug source as inert text, never a broken link", async () => {
    renderView([entry({ id: "pasted", source_repo: "platform/template:ops" })]);

    expect(await screen.findByText("platform/template:ops")).toBeTruthy();
    expect(screen.queryByRole("link", { name: /platform\/template:ops/ })).toBeNull();
  });
});
