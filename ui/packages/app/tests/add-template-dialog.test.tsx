import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";
import { SOURCE_KIND_GITHUB } from "../lib/types";
import { routerRefresh, resetCommonMocks } from "./helpers/dashboard-mocks";

const { onboardTemplateActionMock, captureProductEventMock } = vi.hoisted(() => ({
  onboardTemplateActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@/app/(dashboard)/fleets/actions", () => ({
  onboardTemplateAction: onboardTemplateActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

import AddTemplateDialog, {
  CREATE_TEMPLATE_DOC_URL,
} from "../app/(dashboard)/fleets/new/AddTemplateDialog";

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
  render(React.createElement(AddTemplateDialog, { workspaceId: "ws_1" }));
  await user.click(screen.getByRole("button", { name: /^add template$/i }));
  await screen.findByLabelText("Repository");
  return user;
}

function submitDialog() {
  const input = screen.getByLabelText("Repository") as HTMLInputElement;
  if (!input.form) throw new Error("Repository input is missing its form");
  fireEvent.submit(input.form);
}

describe("AddTemplateDialog", () => {
  it("links to the create-template docs from the dialog", async () => {
    await openDialog();
    const link = screen.getByRole("link", { name: "Create a template" });
    expect(link.getAttribute("href")).toBe(CREATE_TEMPLATE_DOC_URL);
  });

  it("rejects an invalid owner/repo source-ref before calling the action", async () => {
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "notarepo");
    submitDialog();
    await screen.findByText(/Use owner\/repo/i);
    expect(onboardTemplateActionMock).not.toHaveBeenCalled();
  });

  it("test_onboard_success_refreshes_gallery and test_onboard_emits_analytics_event", async () => {
    onboardTemplateActionMock.mockResolvedValueOnce({ ok: true, data: onboarded });
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await waitFor(() => {
      expect(onboardTemplateActionMock).toHaveBeenCalledWith("ws_1", {
        source_kind: SOURCE_KIND_GITHUB,
        source_ref: "owner/repo",
      });
    });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.fleet_template_onboarded, {
      workspace_id: "ws_1",
      visibility: "tenant",
      source_kind: SOURCE_KIND_GITHUB,
      outcome: "success",
    });
    expect(screen.queryByLabelText("Repository")).toBeNull();
  });

  it("test_onboard_failure_surfaces_mapped_error", async () => {
    onboardTemplateActionMock.mockResolvedValueOnce({
      ok: false,
      error: "forbidden",
      status: 403,
      errorCode: "UZ-AUTH-022",
    });
    const user = await openDialog();
    await user.type(screen.getByLabelText("Repository"), "owner/repo");
    submitDialog();

    await screen.findByText("You need template access for that");
    expect(screen.getByText("Ask a workspace admin to grant template access.")).toBeTruthy();
    expect(screen.getByText("UZ-AUTH-022")).toBeTruthy();
    expect(screen.getByLabelText("Repository")).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
