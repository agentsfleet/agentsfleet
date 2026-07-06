import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";
import { SOURCE_KIND_GITHUB } from "../lib/types";
import { routerRefresh, resetCommonMocks } from "./helpers/dashboard-mocks";

const { onboardLibraryEntryActionMock, captureProductEventMock } = vi.hoisted(() => ({
  onboardLibraryEntryActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  onboardLibraryEntryAction: onboardLibraryEntryActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

import AddLibraryDialog from "../app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog";
import { CREATE_LIBRARY_DOC_URL } from "../app/(dashboard)/w/[workspaceId]/fleets/new/library-docs";

const onboarded = {
  id: "tmpl_1",
  name: "GitHub PR reviewer",
  visibility: "tenant" as const,
  content_hash: "sha256:abc",
  requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
  support_files: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
});
afterEach(() => cleanup());

async function openDialog() {
  const user = userEvent.setup({ delay: null });
  render(React.createElement(AddLibraryDialog, { workspaceId: "ws_1" }));
  await user.click(screen.getByRole("button", { name: /^create fleet library$/i }));
  await screen.findByLabelText("Repository");
  return user;
}

function submitDialog() {
  const input = screen.getByLabelText("Repository") as HTMLInputElement;
  if (!input.form) throw new Error("Repository input is missing its form");
  fireEvent.submit(input.form);
}

describe("AddLibraryDialog", () => {
  it("links to the create-template docs from the dialog", async () => {
    await openDialog();
    const link = screen.getByRole("link", { name: /^learn more/i });
    expect(link.getAttribute("href")).toBe(CREATE_LIBRARY_DOC_URL);
  });

  it("rejects an invalid owner/repo source-ref before calling the action", async () => {
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "notarepo");
    submitDialog();
    await screen.findByText(/Use owner\/repo/i);
    expect(onboardLibraryEntryActionMock).not.toHaveBeenCalled();
  });

  it("test_onboard_success_refreshes_gallery and test_onboard_emits_analytics_event", async () => {
    onboardLibraryEntryActionMock.mockResolvedValueOnce({ ok: true, data: onboarded });
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), " owner/repo ");
    submitDialog();

    await waitFor(() => {
      expect(onboardLibraryEntryActionMock).toHaveBeenCalledWith("ws_1", {
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: "owner/repo",
      });
    });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.fleet_library_onboarded, {
      workspace_id: "ws_1",
      visibility: "tenant",
      source_kind: SOURCE_KIND_GITHUB,
      outcome: "success",
    });
    expect(screen.queryByLabelText("Repository")).toBeNull();
  });

  it("test_onboard_failure_surfaces_mapped_error", async () => {
    onboardLibraryEntryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "forbidden",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await screen.findByText("You need an additional scope for that");
    expect(screen.getByText("Ask an agentsfleet admin to grant the scope this action requires.")).toBeTruthy();
    expect(screen.getByText("UZ-AUTH-022")).toBeTruthy();
    expect(screen.getByLabelText("Repository")).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("shows pending state while adding a template", async () => {
    let finishAction: ((value: typeof onboarded) => void) | undefined;
    onboardLibraryEntryActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        finishAction = (value) => resolve({ ok: true, data: value });
      }),
    );
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await screen.findByText("Creating fleet library");
    expect(
      (screen.getByRole("button", { name: /creating fleet library create/i }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);

    finishAction?.(onboarded);
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
  });

  it("resets pending state when the dialog closes before the action resolves", async () => {
    let finishAction: ((value: typeof onboarded) => void) | undefined;
    onboardLibraryEntryActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        finishAction = (value) => resolve({ ok: true, data: value });
      }),
    );
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await screen.findByText("Creating fleet library");
    await user.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Create fleet library" })).toBeNull());
    finishAction?.(onboarded);

    await user.click(screen.getByRole("button", { name: /^create fleet library$/i }));
    const dialog = await screen.findByRole("dialog", { name: "Create fleet library" });
    expect(
      (within(dialog).getByRole("button", { name: /^create$/i }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("renders fallback errors without optional body or code rows", async () => {
    onboardLibraryEntryActionMock.mockResolvedValueOnce({
      ok: false,
      error: "repo not found",
      status: 404,
    });
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await screen.findByText("Couldn't create the fleet library — repo not found.");
    expect(screen.queryByText("Ask an agentsfleet admin to grant the scope this action requires.")).toBeNull();
    expect(screen.queryByText(/^UZ-/)).toBeNull();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
