import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush } from "./helpers/dashboard-mocks";
import { createWorkspaceActionMock, resetDashboardMocks } from "./helpers/dashboard-app-mocks";

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const helpers = await import("./helpers/dashboard-mocks");
  return { ...helpers.designSystemCore(await orig<Record<string, unknown>>()) };
});
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(cleanup);

describe("CreateWorkspaceDialog component", () => {
  async function renderDialog(
    props: {
      open?: boolean;
      onOpenChange?: (open: boolean) => void;
      onCreated?: (workspaceName: string) => void;
    } = {},
  ) {
    const onOpenChange = props.onOpenChange ?? vi.fn();
    const { default: CreateWorkspaceDialog } = await import("../components/layout/CreateWorkspaceDialog");
    render(React.createElement(CreateWorkspaceDialog, {
      open: props.open ?? true,
      onOpenChange,
      onCreated: props.onCreated,
    } as never));
    return { onOpenChange };
  }

  it("submits the trimmed name, then closes and routes to the new workspace on success", async () => {
    const user = userEvent.setup({ delay: null });
    const onCreated = vi.fn();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_x", name: "acme-prod" },
    });
    const { onOpenChange } = await renderDialog({ onCreated });
    await user.type(screen.getByTestId("workspace-name-input"), "  acme-prod  ");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() => expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "acme-prod" }));
    expect(onCreated).toHaveBeenCalledWith("acme-prod");
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(routerPush).toHaveBeenCalledWith("/w/ws_x");
  });

  it("explains how workspaces organize tenant resources", async () => {
    await renderDialog();
    expect(screen.getByText(
      "Use workspaces to organize fleets, teammates, and credentials within your tenant. Leave the name blank to generate one.",
    )).toBeTruthy();
  });

  it("omits a blank name so the server generates one", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_y", name: "auto-gen" },
    });
    await renderDialog();
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() => expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: undefined }));
  });

  it("shows the mapped error and stays open when the action fails", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    const { onOpenChange } = await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "x");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() => expect(screen.getByTestId("workspace-create-error")).toBeTruthy());
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("submits when Enter is pressed inside the name field", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_z", name: "via-enter" },
    });
    await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "via-enter{Enter}");
    await waitFor(() => expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "via-enter" }));
  });

  it("Cancel closes the dialog without calling the action", async () => {
    const user = userEvent.setup({ delay: null });
    const { onOpenChange } = await renderDialog();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(createWorkspaceActionMock).not.toHaveBeenCalled();
  });

  it("ignores a second Enter submit while the first is still in flight", async () => {
    const user = userEvent.setup({ delay: null });
    let release: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
    await renderDialog();
    const input = screen.getByTestId("workspace-name-input");
    await user.type(input, "ws{Enter}");
    await user.type(input, "{Enter}");
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
    release({ ok: true, data: { workspace_id: "ws_p", name: "ws" } });
  });

  it("clears a typed name when the dialog closes", async () => {
    const user = userEvent.setup({ delay: null });
    const { default: CreateWorkspaceDialog } = await import("../components/layout/CreateWorkspaceDialog");
    const onOpenChange = vi.fn();
    const { rerender } = render(React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never));
    await user.type(screen.getByTestId("workspace-name-input"), "draft-name");
    rerender(React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never));
    rerender(React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never));
    expect((screen.getByTestId("workspace-name-input") as HTMLInputElement).value).toBe("");
  });

  it("drops a stale error when the dialog closes", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    const { default: CreateWorkspaceDialog } = await import("../components/layout/CreateWorkspaceDialog");
    const onOpenChange = vi.fn();
    const { rerender } = render(React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never));
    await user.type(screen.getByTestId("workspace-name-input"), "x");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() => expect(screen.getByTestId("workspace-create-error")).toBeTruthy());
    rerender(React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never));
    rerender(React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never));
    expect(screen.queryByTestId("workspace-create-error")).toBeNull();
  });
});
