import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { PlatformCatalogEntry } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import EditFleetDialog from "./EditFleetDialog";

// The pencil. The operator owns the fields no bundle can supply (description,
// credential reasons) plus the row's identity — name, repository, ref — all on
// one ownership line. The server keeps the operator-owned fields out of the refetch
// upsert so an edit here survives the next `Fetch update` — and a CHANGED source
// discards the stored bundle server-side, so the dialog's job is to send only
// what actually moved and to say what a source change costs before Save.
const patchPlatformLibraryActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  patchPlatformLibraryAction: (...args: unknown[]) => patchPlatformLibraryActionMock(...args),
}));

const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
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
  etag: '"catalog-v1"',
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
  // resetAllMocks, not clearAllMocks: moved-only saves mean a test's queued
  // mockResolvedValueOnce can go UNCONSUMED (zero-edit save never hits the
  // wire), and clearAllMocks leaves that stale once-value to poison the next
  // test's first call.
  vi.resetAllMocks();
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
      // moved-only: untouched reasons stay off the wire entirely.
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("github-pr-reviewer", {
        description: "Reviews your pull requests and comments.",
      }, ENTRY.etag),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("keeps the dialog-open ETag when the table revalidates underneath it", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });
    const onOpenChange = vi.fn();
    const view = render(
      <TooltipProvider>
        <EditFleetDialog entry={ENTRY} open onOpenChange={onOpenChange} />
      </TooltipProvider>,
    );

    await user.type(screen.getByLabelText("Description"), " locally edited");
    view.rerender(
      <TooltipProvider>
        <EditFleetDialog
          entry={{ ...ENTRY, description: "Someone else's copy", etag: '"catalog-v2"' }}
          open
          onOpenChange={onOpenChange}
        />
      </TooltipProvider>,
    );
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith(
        ENTRY.id,
        { description: "Reviews pull requests. locally edited" },
        ENTRY.etag,
      ),
    );
  });

  it("keeps the dialog open and shows the mapped error when the save fails", async () => {
    const user = userEvent.setup();
    // moved-only: a zero-edit save is a close, so make an edit to reach the wire.
    patchPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "insufficient scope",
      errorCode: "UZ-AUTH-022",
    });
    const onOpenChange = renderDialog();
    await user.type(screen.getByLabelText("Description"), " x");

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
      // moved-only: the untouched description stays off the wire.
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith("github-pr-reviewer", {
        required_credentials_reasons: { github: "review your pull requests" },
      }, ENTRY.etag),
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
    await user.type(screen.getByLabelText("Description"), " x");

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByLabelText("Description")).toBeTruthy();
  });

  // ── The identity fields ───────────────────────────────────────────────────

  // The client half of the moved-fields rule: the dialog holds every field in state, so a
  // copy-only save could naively re-send the repository the row already has —
  // and re-sending it as "changed" is what would withdraw a live fleet. Only
  // what moved goes on the wire.
  it("sends only the fields that moved — an untouched source is never re-sent", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog();

    await user.clear(screen.getByLabelText("Description"));
    await user.type(screen.getByLabelText("Description"), "sharper copy");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(patchPlatformLibraryActionMock).toHaveBeenCalled());
    const body = patchPlatformLibraryActionMock.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(body.description).toBe("sharper copy");
    expect("source_repo" in body).toBe(false);
    expect("source_ref" in body).toBe(false);
    expect("name" in body).toBe(false);
    // Untouched reasons stay OFF the wire too — a stale dialog must not
    // resurrect keys a refetch pruned, nor clobber another operator's copy.
    expect("required_credentials_reasons" in body).toBe(false);
  });

  it("includes the name only when it moved, and persists it", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog();

    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Reviewer");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith(
        "github-pr-reviewer",
        expect.objectContaining({ name: "Reviewer" }),
        ENTRY.etag,
      ),
    );
  });

  // The one edit with a blast radius gets its warning BEFORE Save, while the
  // operator can still change their mind — not a surprise after.
  it("warns that repointing the source discards the bundle, before saving", async () => {
    const user = userEvent.setup();
    renderDialog();

    expect(screen.queryByTestId("source-warning")).toBeNull();

    const repo = screen.getByLabelText("Repository");
    await user.clear(repo);
    await user.type(repo, "agentsfleet/other-repo");

    expect(await screen.findByTestId("source-warning")).toBeTruthy();
  });

  it("blocks save while the repository is not owner/repo shaped", async () => {
    const user = userEvent.setup();
    renderDialog();

    const repo = screen.getByLabelText("Repository");
    await user.clear(repo);
    await user.type(repo, "no-slash");

    expect((screen.getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(patchPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // A declared credential with no reason is a credential the
  // install gate will ask for and refuse to explain. The dialog is the only
  // place an operator could ever find that out.
  it("names the credential the install gate cannot explain", () => {
    renderDialog({ ...ENTRY, required_credentials_reasons: {} });
    expect(screen.getByTestId("reason-missing-github")).toBeTruthy();
  });

  it("does not flag a credential whose copy exists", () => {
    renderDialog();
    expect(screen.queryByTestId("reason-missing-github")).toBeNull();
  });

  // Repointing is the one edit that withdraws a live fleet from every gallery —
  // the ops signal. Copy edits are deliberately uninstrumented.
  it("records a source repoint as an operator event, without the repository value", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog({ ...ENTRY, visibility: "public" });

    const repo = screen.getByLabelText("Repository");
    await user.clear(repo);
    await user.type(repo, "agentsfleet/moved");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_library_source_changed, {
        entry_id: "github-pr-reviewer",
        field: "repo",
        was_published: true,
        outcome: "success",
      }),
    );
    // The repository VALUE never rides the event — it can carry a private org.
    const props = JSON.stringify(captureProductEventMock.mock.calls);
    expect(props).not.toContain("agentsfleet/moved");
  });

  // The other half of the ops signal: a repoint the server REFUSED is still an
  // operator action, and the event must say so. Without this the funnel silently
  // reports only the repoints that worked — a repoint nobody could complete would
  // look like a repoint nobody attempted.
  it("records a REFUSED source repoint as a failure outcome, and keeps the dialog open", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "This entry has no bundle. Fetch it from its repository first, then publish.",
      errorCode: "UZ-CATALOG-002",
    });
    const onOpenChange = renderDialog({ ...ENTRY, visibility: "public" });

    const repo = screen.getByLabelText("Repository");
    await user.clear(repo);
    await user.type(repo, "agentsfleet/moved");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_library_source_changed, {
        entry_id: "github-pr-reviewer",
        field: "repo",
        was_published: true,
        outcome: "failure",
      }),
    );
    // The refusal is surfaced, and the operator's edit is not thrown away.
    expect(await screen.findByText("UZ-CATALOG-002")).toBeTruthy();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });

  it("records a ref pin as its own field, and sends only the ref", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog();

    const ref = screen.getByLabelText("Ref");
    await user.clear(ref);
    await user.type(ref, "v2");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(patchPlatformLibraryActionMock).toHaveBeenCalledWith(
        "github-pr-reviewer",
        expect.objectContaining({ source_ref: "v2" }),
        ENTRY.etag,
      ),
    );
    const body = patchPlatformLibraryActionMock.mock.calls[0]?.[1] as Record<string, unknown>;
    expect("source_repo" in body).toBe(false);
    await waitFor(() =>
      expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_library_source_changed,
        expect.objectContaining({ field: "ref", outcome: "success" })),
    );
  });

  // The ref rides the same charset rules as a repository segment, and the raw
  // value is what a save sends — so a slash or a stray space blocks HERE, not
  // as a failed round-trip to the server's refusal.
  it("blocks a save while the ref is malformed", async () => {
    const user = userEvent.setup();
    renderDialog();

    const ref = screen.getByLabelText("Ref");
    await user.clear(ref);
    await user.type(ref, "feature/x");

    expect(screen.getByRole("button", { name: /^save$/i })).toHaveProperty("disabled", true);
    expect(patchPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  it("does not emit the source event on a copy-only save", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog();

    await user.type(screen.getByLabelText("Description"), " sharper");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(patchPlatformLibraryActionMock).toHaveBeenCalled());
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  // A template/upload-sourced row carries a source that is not owner/repo
  // shaped. Its UNTOUCHED source is never sent, so it is never re-validated —
  // the row stays copy-editable instead of being locked out of its own dialog.
  it("keeps a non-slug-source row copy-editable — untouched source is not validated", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog({ ...ENTRY, source_repo: "pasted-template" });

    await user.clear(screen.getByLabelText("Description"));
    await user.type(screen.getByLabelText("Description"), "copy edit");
    expect((screen.getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(false);
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(patchPlatformLibraryActionMock).toHaveBeenCalled());
    const body = patchPlatformLibraryActionMock.mock.calls[0]?.[1] as Record<string, unknown>;
    expect("source_repo" in body).toBe(false);
  });

  // One save = one event. A both-halves repoint (fork + release tag) must not
  // double-count the withdrawal it causes.
  it("emits a single event with field 'both' when repo and ref move together", async () => {
    const user = userEvent.setup();
    patchPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true });
    renderDialog();

    const repo = screen.getByLabelText("Repository");
    await user.clear(repo);
    await user.type(repo, "agentsfleet/fork");
    const ref = screen.getByLabelText("Ref");
    await user.clear(ref);
    await user.type(ref, "v2");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalledTimes(1));
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.platform_library_source_changed,
      expect.objectContaining({ field: "both" }));
  });

  it("a save with zero moves closes without writing", async () => {
    const user = userEvent.setup();
    const onOpenChange = renderDialog();

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(patchPlatformLibraryActionMock).not.toHaveBeenCalled();
  });
});
