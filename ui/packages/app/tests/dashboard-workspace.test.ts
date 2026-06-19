import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh } from "./helpers/dashboard-mocks";
import { resetDashboardMocks, setActiveWorkspaceMock, createWorkspaceActionMock } from "./helpers/dashboard-app-mocks";

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemDropdown() };
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/agents", async () => (await import("./helpers/dashboard-app-mocks")).agentsApiMock());
vi.mock("@/app/(dashboard)/agents/actions", async () => (await import("./helpers/dashboard-app-mocks")).agentActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderSelector", async () => (await import("./helpers/dashboard-app-mocks")).providerSelectorMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/lib/api/credentials", async () => (await import("./helpers/dashboard-app-mocks")).credentialsApiMock());
vi.mock("@/app/(dashboard)/credentials/components/AddCredentialForm", async () => (await import("./helpers/dashboard-app-mocks")).addCredentialFormMock());
vi.mock("@/app/(dashboard)/credentials/components/CredentialsList", async () => (await import("./helpers/dashboard-app-mocks")).credentialsListMock());
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("WorkspaceSwitcher component", () => {
  async function renderSwitcher(props: {
    workspaces?: Array<{ id: string; name: string | null }>;
    activeId?: string | null;
    onSwitch?: (id: string) => void | Promise<void>;
  } = {}) {
    const { default: WorkspaceSwitcher } = await import(
      "../components/layout/WorkspaceSwitcher"
    );
    render(
      React.createElement(WorkspaceSwitcher, {
        workspaces: props.workspaces ?? [
          { id: "ws_1", name: "Alpha" },
          { id: "ws_2", name: "Beta" },
        ],
        activeId: props.activeId ?? "ws_1",
        onSwitch: props.onSwitch ?? setActiveWorkspaceMock,
      } as never),
    );
  }

  it("still renders with a New workspace affordance when workspaces is empty", async () => {
    render(
      React.createElement(
        (await import("../components/layout/WorkspaceSwitcher")).default,
        { workspaces: [], activeId: null, onSwitch: setActiveWorkspaceMock } as never,
      ),
    );
    // The empty case is exactly when create matters most — switcher must show.
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("No workspace");
    expect(screen.getByTestId("workspace-new")).toBeTruthy();
  });

  it("opens the create dialog from the New workspace item", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await waitFor(() => expect(screen.getByTestId("workspace-name-input")).toBeTruthy());
  });

  it("bounds the workspace menu so create actions remain reachable", async () => {
    const manyWorkspaces = Array.from({ length: 32 }, (_, index) => ({
      id: `ws_${index}`,
      name: `Workspace ${index}`,
    }));
    const { container } = render(
      React.createElement(
        (await import("../components/layout/WorkspaceSwitcher")).default,
        {
          workspaces: manyWorkspaces,
          activeId: "ws_0",
          onSwitch: setActiveWorkspaceMock,
        } as never,
      ),
    );
    const menu = container.querySelector("[data-dropdown-content]");
    expect(menu?.className).toContain("max-h-96");
    expect(menu?.className).toContain("overflow-y-auto");
  });

  it("renders the active workspace label", async () => {
    await renderSwitcher();
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("falls back to id when name is null", async () => {
    await renderSwitcher({
      workspaces: [{ id: "ws_only", name: null }],
      activeId: "ws_only",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("ws_only");
  });

  it("falls back to the first workspace when activeId is unknown", async () => {
    await renderSwitcher({
      workspaces: [
        { id: "ws_a", name: "Alpha" },
        { id: "ws_b", name: "Beta" },
      ],
      activeId: "ws_unknown",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("picking a different workspace invokes the onSwitch callback", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // Second item = Beta (different from active ws_1)
    await user.click(items[1]!);
    await waitFor(() =>
      expect(setActiveWorkspaceMock).toHaveBeenCalledWith("ws_2"),
    );
    expect(routerRefresh).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy(),
    );
  });

  it("picking the active workspace is a no-op", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // First item = Alpha (same as active)
    await user.click(items[0]!);
    // Give transition a tick
    await new Promise((r) => setTimeout(r, 10));
    expect(setActiveWorkspaceMock).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("navigates to Settings from Manage workspace", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-manage"));
    expect(routerPush).toHaveBeenCalledWith("/settings");
  });
});

// ── CreateWorkspaceDialog ───────────────────────────────────────────────────

describe("CreateWorkspaceDialog component", () => {
  async function renderDialog(
    props: {
      open?: boolean;
      onOpenChange?: (open: boolean) => void;
      onCreated?: (workspaceName: string) => void;
    } = {},
  ) {
    const onOpenChange = props.onOpenChange ?? vi.fn();
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    render(
      React.createElement(CreateWorkspaceDialog, {
        open: props.open ?? true,
        onOpenChange,
        onCreated: props.onCreated,
      } as never),
    );
    return { onOpenChange };
  }

  it("submits the trimmed name, then closes and refreshes on success", async () => {
    const user = userEvent.setup({ delay: null });
    const onCreated = vi.fn();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_x", name: "acme-prod" },
    });
    const { onOpenChange } = await renderDialog({ onCreated });
    await user.type(screen.getByTestId("workspace-name-input"), "  acme-prod  ");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "acme-prod" }),
    );
    expect(onCreated).toHaveBeenCalledWith("acme-prod");
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("omits a blank name so the server generates a Heroku-style one", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_y", name: "auto-gen" },
    });
    await renderDialog();
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: undefined }),
    );
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
    await waitFor(() =>
      expect(screen.getByTestId("workspace-create-error")).toBeTruthy(),
    );
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("submits when Enter is pressed inside the name field", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_z", name: "via-enter" },
    });
    await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "via-enter{Enter}");
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "via-enter" }),
    );
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
    let release: (v: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () => new Promise((r) => { release = r; }), // stays pending until released
    );
    await renderDialog();
    const input = screen.getByTestId("workspace-name-input");
    await user.type(input, "ws{Enter}"); // first submit → useTransition pending
    await user.type(input, "{Enter}"); // second Enter hits the `if (pending) return` guard
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
    release({ ok: true, data: { workspace_id: "ws_p", name: "ws" } });
  });

  it("clears a typed-but-unsaved name when the dialog closes, so a reopen starts fresh", async () => {
    const user = userEvent.setup({ delay: null });
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    const onOpenChange = vi.fn();
    const { rerender } = render(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    await user.type(screen.getByTestId("workspace-name-input"), "draft-name");
    expect(
      (screen.getByTestId("workspace-name-input") as HTMLInputElement).value,
    ).toBe("draft-name");
    // Close (open→false): the effect cleanup resets the form while the
    // component stays mounted.
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never),
    );
    // Reopen: the name field is blank again, not the abandoned "draft-name".
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    expect(
      (screen.getByTestId("workspace-name-input") as HTMLInputElement).value,
    ).toBe("");
  });

  it("drops a stale error when the dialog closes, so a reopen starts clean", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    const onOpenChange = vi.fn();
    const { rerender } = render(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    await user.type(screen.getByTestId("workspace-name-input"), "x");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-create-error")).toBeTruthy(),
    );
    // Close then reopen: the cleanup must drop the error banner, not carry the
    // failed-attempt message into a fresh session.
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never),
    );
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    expect(screen.queryByTestId("workspace-create-error")).toBeNull();
  });
});
