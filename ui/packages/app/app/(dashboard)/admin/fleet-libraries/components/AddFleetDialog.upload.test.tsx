import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { EVENTS } from "@/lib/analytics/events";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "@/components/domain/fleet-library/bundle-files";
import AddFleetDialog from "./AddFleetDialog";

// The platform catalog onboards through the same importer as the workspace
// dialog, so it has always been able to take a pasted bundle — `resolve.resolve`
// routes `upload` on both tiers. Only the operator's screen was GitHub-only.
// These cover the second source; AddFleetDialog.test.tsx covers the first.

const onboardPlatformLibraryActionMock = vi.fn();
const captureProductEventMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
}));

const REPO = "agentsfleet/platform-ops";
const STORED_REF = "release-2";
const SKILL_BODY = "---\nname: incident-responder\n---\nBody.";
const TRIGGER_BODY = "---\nname: incident-responder\nx-agentsfleet:\n---";
const UPLOAD_TAB = "Upload from computer";

const ENTRY = {
  id: "incident-responder",
  name: "Incident responder",
  visibility: "platform" as const,
  content_hash: "sha256:abc123",
  requirements: { credentials: [], tools: ["http_request"], network_hosts: [], trigger_present: true },
};

function Harness({ prefillRepo, prefillRef }: { prefillRepo?: string; prefillRef?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <TooltipProvider>
      <Button type="button" onClick={() => setOpen(true)}>
        open
      </Button>
      <AddFleetDialog open={open} onOpenChange={setOpen} prefillRepo={prefillRepo} prefillRef={prefillRef} />
    </TooltipProvider>
  );
}

async function openUploadTab(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: /^open$/i }));
  await user.click(await screen.findByRole("tab", { name: UPLOAD_TAB }));
  return {
    skill: await screen.findByLabelText(SKILL_FILE_NAME),
    trigger: screen.getByLabelText(TRIGGER_FILE_NAME),
  };
}

async function submitBundle(user: ReturnType<typeof userEvent.setup>) {
  const { skill, trigger } = await openUploadTab(user);
  await user.type(skill, SKILL_BODY);
  await user.type(trigger, TRIGGER_BODY);
  await user.click(screen.getByRole("button", { name: "Create" }));
}

beforeEach(() => {
  vi.clearAllMocks();
  render(<Harness />);
});

afterEach(() => {
  cleanup();
});

describe("AddFleetDialog upload source", () => {
  it("offers a local bundle beside the repository", async () => {
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /^open$/i }));

    expect(await screen.findByRole("tab", { name: "GitHub" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: UPLOAD_TAB })).toBeTruthy();
  });

  it("sends both bundle bodies and no repository", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });

    await submitBundle(user);

    await waitFor(() =>
      expect(onboardPlatformLibraryActionMock).toHaveBeenCalledWith({
        source_kind: "upload",
        skill_markdown: SKILL_BODY,
        trigger_markdown: TRIGGER_BODY,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("refuses a half-filled bundle before it reaches the importer", async () => {
    const user = userEvent.setup();
    const { skill } = await openUploadTab(user);
    await user.type(skill, SKILL_BODY);
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByText(`Add the ${TRIGGER_FILE_NAME} body`)).toBeTruthy();
    expect(onboardPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // An upload stores no repository, so a name a GitHub-sourced row already owns
  // still collides. Without `replace` on the retry the button would re-earn the
  // same refusal forever.
  it("retries an upload collision with replace once the operator confirms", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock
      .mockResolvedValueOnce({ ok: false, error: "name taken", errorCode: "UZ-CATALOG-004" })
      .mockResolvedValueOnce({ ok: true, data: ENTRY });

    await submitBundle(user);
    await user.click(await screen.findByRole("button", { name: /replace anyway/i }));

    await waitFor(() =>
      expect(onboardPlatformLibraryActionMock).toHaveBeenLastCalledWith({
        source_kind: "upload",
        skill_markdown: SKILL_BODY,
        trigger_markdown: TRIGGER_BODY,
        replace: true,
      }),
    );
  });

  it("records which source an add came from", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({ ok: true, data: ENTRY });

    await submitBundle(user);

    await waitFor(() => expect(captureProductEventMock).toHaveBeenCalled());
    const [event, props] = captureProductEventMock.mock.calls[0] ?? [];
    expect(event).toBe(EVENTS.platform_library_onboarded);
    expect(props).toEqual({ source_kind: "upload", outcome: "success", entry_id: ENTRY.id });
  });

  // The answer to an in-flight request lands against whatever the form holds when
  // it arrives. Leaving the source switchable meanwhile lets a refusal earned by
  // the repository be answered on behalf of an upload.
  it("locks the source while a submit is in flight", async () => {
    const user = userEvent.setup();
    let release: (v: unknown) => void = () => {};
    onboardPlatformLibraryActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        release = resolve;
      }),
    );

    await user.click(screen.getByRole("button", { name: /^open$/i }));
    await user.type(await screen.findByLabelText(/repository/i), REPO);
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() =>
      expect(screen.getByRole("tab", { name: UPLOAD_TAB }).hasAttribute("disabled")).toBe(true),
    );
    expect(screen.getByRole("tab", { name: "GitHub" }).hasAttribute("disabled")).toBe(true);

    release({ ok: true, data: ENTRY });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  // The Replace button retries whatever the form currently holds. Left standing
  // across a tab switch it would offer to overwrite a name on behalf of a source
  // the collision was never reported for.
  it("withdraws the replace offer when the operator changes source", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "name taken",
      errorCode: "UZ-CATALOG-004",
    });

    await user.click(screen.getByRole("button", { name: /^open$/i }));
    await user.type(await screen.findByLabelText(/repository/i), REPO);
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("button", { name: /replace anyway/i })).toBeTruthy();

    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /replace anyway/i })).toBeNull(),
    );
  });

  // Closing is how an operator abandons a bundle. Reopening on to the previous
  // one would offer to publish bytes they walked away from.
  it("clears an abandoned bundle so a reopen starts empty", async () => {
    const user = userEvent.setup();
    const { skill } = await openUploadTab(user);
    await user.type(skill, SKILL_BODY);
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    const reopened = await openUploadTab(user);
    expect((reopened.skill as HTMLTextAreaElement).value).toBe("");
    expect((reopened.trigger as HTMLTextAreaElement).value).toBe("");
    expect(onboardPlatformLibraryActionMock).not.toHaveBeenCalled();
  });

  // Switching tabs must drop the other source's refusal, or the upload form keeps
  // explaining a failure the repository earned.
  it("clears a server error when the operator changes source", async () => {
    const user = userEvent.setup();
    onboardPlatformLibraryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "no SKILL.md at the repository root",
      errorCode: "UZ-BUNDLE-002",
    });

    await user.click(screen.getByRole("button", { name: /^open$/i }));
    await user.type(await screen.findByLabelText(/repository/i), REPO);
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("UZ-BUNDLE-002")).toBeTruthy();

    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));

    await waitFor(() => expect(screen.queryByText("UZ-BUNDLE-002")).toBeNull());
  });
});

describe("AddFleetDialog refetch source", () => {
  // Fetch-update re-reads the row's own source. A second source on that screen
  // would be an offer to change what is being re-read — a different write.
  it("offers no choice of source", async () => {
    cleanup();
    const user = userEvent.setup();
    render(<Harness prefillRepo={REPO} prefillRef={STORED_REF} />);

    await user.click(screen.getByRole("button", { name: /^open$/i }));

    expect((await screen.findByLabelText(/repository/i)).hasAttribute("readonly")).toBe(true);
    expect(screen.queryByRole("tab", { name: UPLOAD_TAB })).toBeNull();
    expect(screen.queryByLabelText(SKILL_FILE_NAME)).toBeNull();
  });
});
