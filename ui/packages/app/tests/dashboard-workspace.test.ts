import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  routerPush,
  routerRefresh,
  usePathname,
} from "./helpers/dashboard-mocks";
import {
  resetDashboardMocks,
  createWorkspaceActionMock,
} from "./helpers/dashboard-app-mocks";
import { EVENTS } from "@/lib/analytics/events";

const captureProductEventMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", async (orig) => {
  const actual = await orig<typeof import("@/lib/analytics/posthog")>();
  return { ...actual, captureProductEvent: captureProductEventMock };
});

vi.mock("next/navigation", async () =>
  (await import("./helpers/dashboard-mocks")).nextNavigationMock(),
);
vi.mock("next/link", async () =>
  (await import("./helpers/dashboard-mocks")).nextLinkMock(),
);
vi.mock("@clerk/nextjs", async () =>
  (await import("./helpers/dashboard-mocks")).clerkMock(),
);
vi.mock("@clerk/nextjs/server", async () =>
  (await import("./helpers/dashboard-mocks")).clerkServerMock(),
);
vi.mock("@/lib/workspace", async () =>
  (await import("./helpers/dashboard-mocks")).workspaceMock(),
);
vi.mock("lucide-react", async () =>
  (await import("./helpers/dashboard-mocks")).lucideMock(),
);
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return {
    ...h.designSystemCore(await orig<Record<string, unknown>>()),
    ...h.designSystemDropdown(),
  };
});

vi.mock("@/lib/api/fleets", async () =>
  (await import("./helpers/dashboard-app-mocks")).fleetsApiMock(),
);
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/dashboard-app-mocks")).fleetActionsMock(),
);
vi.mock("@/lib/api/tenant_billing", async () =>
  (await import("./helpers/dashboard-app-mocks")).tenantBillingMock(),
);
vi.mock("@/lib/api/tenant_provider", async () =>
  (await import("./helpers/dashboard-app-mocks")).tenantProviderMock(),
);
vi.mock(
  "@/app/(dashboard)/settings/billing/components/BillingBalanceCard",
  async () =>
    (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock(),
);
vi.mock(
  "@/app/(dashboard)/settings/billing/components/BillingUsageTab",
  async () =>
    (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock(),
);
vi.mock("@/lib/api/events", async () =>
  (await import("./helpers/dashboard-app-mocks")).eventsMock(),
);
vi.mock("@/lib/api/secrets", async () =>
  (await import("./helpers/dashboard-app-mocks")).secretsApiMock(),
);
vi.mock(
  "@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm",
  async () =>
    (await import("./helpers/dashboard-app-mocks")).addSecretFormMock(),
);
vi.mock(
  "@/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList",
  async () => (await import("./helpers/dashboard-app-mocks")).secretsListMock(),
);
vi.mock("@/app/(dashboard)/actions", async () =>
  (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock(),
);

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("WorkspaceSwitcher component", () => {
  async function activateWorkspaceMenu() {
    await act(async () => {
      fireEvent.click(screen.getByTestId("workspace-switcher"));
      await import("../components/layout/WorkspaceSwitcherMenu");
      await Promise.resolve();
    });
    expect(screen.getByTestId("workspace-new")).toBeTruthy();
  }

  async function renderSwitcher(
    props: {
      workspaces?: Array<{ id: string; name: string | null }>;
    } = {},
  ) {
    const [{ default: WorkspaceSwitcher }, { WorkspaceCreationProvider }] =
      await Promise.all([
        import("../components/layout/WorkspaceSwitcher"),
        import("../components/layout/WorkspaceCreationProvider"),
      ]);
    const view = render(
      React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(WorkspaceSwitcher, {
          workspaces: props.workspaces ?? [
            { id: "ws_1", name: "Alpha" },
            { id: "ws_2", name: "Beta" },
          ],
        } as never),
      ),
    );
    await activateWorkspaceMenu();
    return view;
  }

  function deferWorkspaceAction() {
    let resolve: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((next) => {
          resolve = next;
        }),
    );
    return (value: unknown) => act(async () => resolve(value));
  }

  async function enterWorkspaceName(name = "test-workspace") {
    fireEvent.change(await screen.findByTestId("workspace-name-input"), {
      target: { value: name },
    });
  }

  it("still renders with a Create workspace affordance when workspaces is empty", async () => {
    usePathname.mockReturnValue("/");
    await renderSwitcher({ workspaces: [] });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain(
      "No workspace",
    );
    expect(screen.getByTestId("workspace-new")).toBeTruthy();
  });

  it("opens the create dialog from the Create workspace item", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-name-input")).toBeTruthy(),
    );
  });

  it("keeps workspace rows bounded without scrolling the create action", async () => {
    const manyWorkspaces = Array.from({ length: 32 }, (_, index) => ({
      id: `ws_${index}`,
      name: `Workspace ${index}`,
    }));
    const { container } = await renderSwitcher({
      workspaces: manyWorkspaces,
    });
    const menu = container.querySelector("[data-dropdown-content]");
    const list = screen.getByTestId("workspace-list-scroll");
    expect(menu?.className).toContain("overflow-hidden");
    expect(list.className).toContain("max-h-80");
    expect(list.className).toContain("overflow-y-auto");
    expect(list.contains(screen.getByTestId("workspace-new"))).toBe(false);
  });

  it("renders the active workspace label", async () => {
    await renderSwitcher();
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain(
      "Alpha",
    );
  });

  it("uses the oldest visible workspace when no route is active", async () => {
    usePathname.mockReturnValue("/");
    await renderSwitcher();
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain(
      "Alpha",
    );
  });

  it("uses a calm label when a workspace name is absent", async () => {
    usePathname.mockReturnValue("/");
    await renderSwitcher({
      workspaces: [{ id: "ws_only", name: null }],
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain(
      "Unnamed workspace",
    );
  });

  it("shows a safe routed label when the bounded list omits the active workspace", async () => {
    usePathname.mockReturnValue("/w/ws_unknown/fleets");
    await renderSwitcher({
      workspaces: [
        { id: "ws_a", name: "Alpha" },
        { id: "ws_b", name: "Beta" },
      ],
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain(
      "Current workspace",
    );
    expect(screen.queryByText("ws_unknown")).toBeNull();
    expect(
      screen.getByRole("menuitem", { name: "Current workspace" }),
    ).toBeTruthy();
  });

  it("picking a different workspace navigates to its URL (same sub-path), writes no cookie", async () => {
    const user = userEvent.setup({ delay: null });
    usePathname.mockReturnValue("/w/ws_1/fleets");
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_2/fleets"),
    );
    expect(captureProductEventMock).toHaveBeenCalledWith(
      EVENTS.workspace_switched,
      { workspace_id: "ws_2" },
    );
    await waitFor(() =>
      expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy(),
    );
  });

  it("collapses a resource-detail path to its section on switch (avoids a guaranteed 404)", async () => {
    const user = userEvent.setup({ delay: null });
    usePathname.mockReturnValue("/w/ws_1/fleets/fleet_abc");
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_2/fleets"),
    );
  });

  it("navigates into the displayed fallback workspace from a tenant page", async () => {
    const user = userEvent.setup({ delay: null });
    usePathname.mockReturnValue("/settings/billing");
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[0]!);
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_1/fleets"),
    );
  });

  it("uses a calm fallback in the switch toast when the workspace has no name", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher({
      workspaces: [
        { id: "ws_1", name: "Alpha" },
        { id: "ws_no_name", name: null },
      ],
    });
    const items = screen.getAllByRole("menuitem");
    await user.click(items[1]!);
    await waitFor(() =>
      expect(
        screen.getByText("Workspace changed to Unnamed workspace."),
      ).toBeTruthy(),
    );
  });

  it("truncates long workspace names while preserving the full title", async () => {
    const longName = "A".repeat(128);
    await renderSwitcher({
      workspaces: [{ id: "ws_long", name: longName }],
    });
    const label = screen.getByTitle(longName);
    expect(label.className).toContain("truncate");
    expect(label.className).toContain("min-w-0");
  });

  it("clears the workspace switch toast after the notice timeout", async () => {
    vi.useFakeTimers();
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    fireEvent.click(items[1]!);
    expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2900);
    });
    expect(screen.getByText("Workspace changed to Beta.")).toBeTruthy();
    expect(
      screen.getByTestId("workspace-toast").getAttribute("aria-hidden"),
    ).toBe("true");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(250);
    });
    expect(screen.queryByText("Workspace changed to Beta.")).toBeNull();
  });

  it("picking the active workspace is a no-op", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    await user.click(items[0]!);
    await new Promise((r) => setTimeout(r, 10));
    expect(routerPush).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("has no Manage workspace item — switching/creating are the only actions", async () => {
    await renderSwitcher();
    expect(screen.queryByTestId("workspace-manage")).toBeNull();
  });

  it("creates a workspace from the dropdown item and shows the created toast", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_inline", name: "inline-prod" },
    });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await user.type(
      await screen.findByTestId("workspace-name-input"),
      "inline-prod",
    );
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByText("Workspace created: inline-prod.")).toBeTruthy(),
    );
    expect(screen.getByRole("menuitem", { name: "inline-prod" })).toBeTruthy();
    expect(routerPush).toHaveBeenCalledWith("/w/ws_inline/fleets");
    expect(
      screen.getByTestId("workspace-new").getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("shows a duplicate-name conflict and refreshes the workspace list once", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-WORKSPACE-001",
      error: "A workspace with this name already exists",
    });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    const input = (await screen.findByTestId(
      "workspace-name-input",
    )) as HTMLInputElement;
    await user.type(input, "kept-name");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-create-error").textContent).toMatch(
        /already exists.*refreshing the workspace list.*before retrying/i,
      ),
    );
    expect(input.value).toBe("kept-name");
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
    expect(routerRefresh).toHaveBeenCalledTimes(1);
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("test_react19_transitions_and_optimistic_rollbacks_are_stable", async () => {
    const user = userEvent.setup({ delay: null });
    createWorkspaceActionMock.mockRejectedValueOnce(
      new Error("network unavailable"),
    );
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("network-failure");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-create-error").textContent,
      ).toContain("Couldn't create workspace"),
    );
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("uses an attempt guard to block same-frame duplicate submissions", async () => {
    const release = deferWorkspaceAction();
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("once");
    const form = await screen.findByTestId("workspace-create-form");
    fireEvent.submit(form);
    fireEvent.submit(form);
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
    await release({
      ok: true,
      data: { workspace_id: "ws_once", name: "once" },
    });
  });

  it("keeps creation running after Hide and does not navigate on late success", async () => {
    const release = deferWorkspaceAction();
    const user = userEvent.setup({ delay: null });
    usePathname.mockReturnValue("/w/ws_1/fleets");
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("background");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(
        (screen.getByTestId("workspace-create-submit") as HTMLButtonElement)
          .disabled,
      ).toBe(true),
    );
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(
      screen.getByText("Workspace creation continues in the background."),
    ).toBeTruthy();
    expect(screen.queryByTestId("workspace-name-input")).toBeNull();
    expect(
      screen.getByTestId("workspace-new").getAttribute("aria-disabled"),
    ).toBe("true");

    await release({
      ok: true,
      data: { workspace_id: "ws_background", name: "background" },
    });
    await waitFor(() =>
      expect(screen.getByText("Workspace created: background.")).toBeTruthy(),
    );
    expect(screen.getByRole("menuitem", { name: "background" })).toBeTruthy();
    expect(routerPush).not.toHaveBeenCalled();
    expect(routerRefresh).toHaveBeenCalledOnce();
  });

  it("locks both creation surfaces while a hidden first-workspace request is running", async () => {
    const release = deferWorkspaceAction();
    const [
      { default: WorkspaceSwitcher },
      { default: NoWorkspaceEmptyState },
      { WorkspaceCreationProvider },
    ] = await Promise.all([
      import("../components/layout/WorkspaceSwitcher"),
      import("../components/layout/NoWorkspaceEmptyState"),
      import("../components/layout/WorkspaceCreationProvider"),
    ]);
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(WorkspaceSwitcher, {
            workspaces: [],
          }),
          React.createElement(NoWorkspaceEmptyState),
        ),
      ),
    );
    await activateWorkspaceMenu();

    await user.click(screen.getByTestId("create-first-workspace"));
    await enterWorkspaceName("shared");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(
        (screen.getByTestId("workspace-create-submit") as HTMLButtonElement)
          .disabled,
      ).toBe(true),
    );
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(
      screen.getByTestId("workspace-new").getAttribute("aria-disabled"),
    ).toBe("true");
    await user.click(screen.getByTestId("workspace-new"));
    expect(screen.queryByTestId("workspace-name-input")).toBeNull();
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);

    await release({
      ok: true,
      data: { workspace_id: "ws_shared", name: "shared" },
    });
    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: "shared" })).toBeTruthy(),
    );
    expect(
      screen.getByTestId("workspace-new").getAttribute("aria-disabled"),
    ).toBe("true");
    expect(screen.getByTestId("create-first-workspace").textContent).toBe(
      "Open workspace",
    );
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
  });

  it("locks the empty-state create path while switcher navigation settles", async () => {
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_switcher", name: "switcher" },
    });
    const [
      { default: WorkspaceSwitcher },
      { default: NoWorkspaceEmptyState },
      { WorkspaceCreationProvider },
    ] = await Promise.all([
      import("../components/layout/WorkspaceSwitcher"),
      import("../components/layout/NoWorkspaceEmptyState"),
      import("../components/layout/WorkspaceCreationProvider"),
    ]);
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(WorkspaceSwitcher, {
            workspaces: [],
          }),
          React.createElement(NoWorkspaceEmptyState),
        ),
      ),
    );
    await activateWorkspaceMenu();

    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("switcher");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_switcher/fleets"),
    );
    expect(
      screen.getByTestId("workspace-new").getAttribute("aria-disabled"),
    ).toBe("true");
    expect(screen.getByTestId("create-first-workspace").textContent).toBe(
      "Open workspace",
    );
    await user.click(screen.getByTestId("create-first-workspace"));
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
  });

  it("detaches an unmounted page owner before late completion can navigate", async () => {
    let resolveAction: (value: unknown) => void = () => {};
    usePathname.mockReturnValue("/");
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveAction = resolve;
        }),
    );
    const [
      { default: WorkspaceSwitcher },
      { default: NoWorkspaceEmptyState },
      { WorkspaceCreationProvider },
    ] = await Promise.all([
      import("../components/layout/WorkspaceSwitcher"),
      import("../components/layout/NoWorkspaceEmptyState"),
      import("../components/layout/WorkspaceCreationProvider"),
    ]);
    function Tree({ showEmptyState }: { showEmptyState: boolean }) {
      React.useLayoutEffect(() => {
        if (!showEmptyState) {
          resolveAction({
            ok: true,
            data: { workspace_id: "ws_after_unmount", name: "after-unmount" },
          });
        }
      }, [showEmptyState]);
      return React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(WorkspaceSwitcher, {
            workspaces: [],
          }),
          showEmptyState ? React.createElement(NoWorkspaceEmptyState) : null,
        ),
      );
    }
    const user = userEvent.setup({ delay: null });
    const view = render(React.createElement(Tree, { showEmptyState: true }));
    await activateWorkspaceMenu();
    await user.click(screen.getByTestId("create-first-workspace"));
    await enterWorkspaceName("after-unmount");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(
        (screen.getByTestId("workspace-create-submit") as HTMLButtonElement)
          .disabled,
      ).toBe(true),
    );

    view.rerender(React.createElement(Tree, { showEmptyState: false }));
    await waitFor(() =>
      expect(
        screen.getByText("Workspace created: after-unmount."),
      ).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(
      screen.getByRole("menuitem", { name: "after-unmount" }),
    ).toBeTruthy();
  });

  it("keeps a switcher attempt attached when an empty-state sibling unmounts", async () => {
    const release = deferWorkspaceAction();
    const [
      { default: WorkspaceSwitcher },
      { default: NoWorkspaceEmptyState },
      { WorkspaceCreationProvider },
    ] = await Promise.all([
      import("../components/layout/WorkspaceSwitcher"),
      import("../components/layout/NoWorkspaceEmptyState"),
      import("../components/layout/WorkspaceCreationProvider"),
    ]);
    function Tree({ showEmptyState }: { showEmptyState: boolean }) {
      return React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(WorkspaceSwitcher, {
            workspaces: [],
          }),
          showEmptyState ? React.createElement(NoWorkspaceEmptyState) : null,
        ),
      );
    }
    const user = userEvent.setup({ delay: null });
    const view = render(React.createElement(Tree, { showEmptyState: true }));
    await activateWorkspaceMenu();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("owner");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    view.rerender(React.createElement(Tree, { showEmptyState: false }));

    await release({
      ok: true,
      data: { workspace_id: "ws_owner", name: "owner" },
    });
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_owner/fleets"),
    );
  });

  it("uses the route committed in the same layout before detached completion settles", async () => {
    let resolveAction: (value: unknown) => void = () => {};
    usePathname.mockReturnValue("/");
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveAction = resolve;
        }),
    );
    const [{ default: WorkspaceSwitcher }, { WorkspaceCreationProvider }] =
      await Promise.all([
        import("../components/layout/WorkspaceSwitcher"),
        import("../components/layout/WorkspaceCreationProvider"),
      ]);
    function ReleaseAfterRouteCommit({ release }: { release: boolean }) {
      React.useLayoutEffect(() => {
        if (release) {
          resolveAction({
            ok: true,
            data: { workspace_id: "ws_route_commit", name: "route-commit" },
          });
        }
      }, [release]);
      return null;
    }
    function Tree({ release }: { release: boolean }) {
      return React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(
          React.Fragment,
          null,
          React.createElement(WorkspaceSwitcher, {
            workspaces: [],
          }),
          React.createElement(ReleaseAfterRouteCommit, { release }),
        ),
      );
    }
    const user = userEvent.setup({ delay: null });
    const view = render(React.createElement(Tree, { release: false }));
    await activateWorkspaceMenu();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("route-commit");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await user.click(screen.getByRole("button", { name: "Hide" }));

    usePathname.mockReturnValue("/w/ws_existing/fleets");
    view.rerender(React.createElement(Tree, { release: true }));
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("keeps a late detached failure out of the next dialog session", async () => {
    const release = deferWorkspaceAction();
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("late-failure");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(
        (screen.getByTestId("workspace-create-submit") as HTMLButtonElement)
          .disabled,
      ).toBe(true),
    );
    await user.click(screen.getByRole("button", { name: "Hide" }));
    await user.click(screen.getByTestId("workspace-new"));
    expect(screen.queryByTestId("workspace-name-input")).toBeNull();

    await release({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    await waitFor(() =>
      expect(screen.getByTestId("workspace-toast").textContent).toContain(
        "Your session expired",
      ),
    );
    expect(screen.getByTestId("workspace-toast").textContent).toMatch(
      /refreshing the workspace list.*before retrying/i,
    );
    expect(routerRefresh).toHaveBeenCalledTimes(1);
    await user.click(screen.getByTestId("workspace-new"));
    expect(await screen.findByTestId("workspace-name-input")).toBeTruthy();
    expect(screen.queryByTestId("workspace-create-error")).toBeNull();
  });

  it("restores focus to the workspace switcher after Cancel", async () => {
    const user = userEvent.setup({ delay: null });
    await renderSwitcher();
    const trigger = screen.getByTestId("workspace-switcher");
    await user.click(screen.getByTestId("workspace-new"));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("invalidates completion as the provider layout unmounts", async () => {
    let resolveAction: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveAction = resolve;
        }),
    );
    const [{ default: WorkspaceSwitcher }, { WorkspaceCreationProvider }] =
      await Promise.all([
        import("../components/layout/WorkspaceSwitcher"),
        import("../components/layout/WorkspaceCreationProvider"),
      ]);
    function ReleaseOnLayoutCleanup() {
      React.useLayoutEffect(
        () => () => {
          resolveAction({
            ok: true,
            data: { workspace_id: "ws_late", name: "late" },
          });
        },
        [],
      );
      return React.createElement(WorkspaceSwitcher, {
        workspaces: [],
      } as never);
    }
    const user = userEvent.setup({ delay: null });
    const view = render(
      React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(ReleaseOnLayoutCleanup),
      ),
    );
    await activateWorkspaceMenu();
    await user.click(screen.getByTestId("workspace-new"));
    await enterWorkspaceName("layout-unmount");
    await user.click(await screen.findByTestId("workspace-create-submit"));
    view.unmount();
    await act(async () => {});
    expect(routerPush).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
