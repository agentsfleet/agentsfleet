import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
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
  createWorkspaceActionMock,
  resetDashboardMocks,
} from "./helpers/dashboard-app-mocks";

vi.mock("next/navigation", async () =>
  (await import("./helpers/dashboard-mocks")).nextNavigationMock(),
);
vi.mock("@/app/(dashboard)/actions", async () =>
  (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock(),
);

// The zero-workspace empty state opens the create dialog through a dynamic
// island; stub it to a marker that renders only when `open`, so the click →
// open wiring is observable without pulling next/dynamic into the test.
vi.mock(
  "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic",
  () => ({
    default: ({
      open,
      onOpenChange,
      onSubmit,
      restoreFocus,
    }: {
      open: boolean;
      onOpenChange: (open: boolean) => void;
      onSubmit: (name?: string) => void | Promise<void>;
      restoreFocus: () => void;
    }) =>
      open
        ? React.createElement(
            "div",
            { "data-testid": "create-dialog-open" },
            React.createElement(
              "button",
              {
                type: "button",
                onClick: () => onSubmit("first"),
                "data-testid": "first-submit",
              },
              "Submit",
            ),
            React.createElement(
              "button",
              {
                type: "button",
                onClick: () => {
                  onOpenChange(false);
                  restoreFocus();
                },
                "data-testid": "first-hide",
              },
              "Hide",
            ),
          )
        : null,
  }),
);

beforeEach(() => {
  vi.clearAllMocks();
  window.sessionStorage.clear();
  resetDashboardMocks();
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("DashboardError boundary", () => {
  it("renders the retry surface and calls reset on click", async () => {
    const reset = vi.fn();
    const { default: DashboardError } =
      await import("../app/(dashboard)/error");
    render(
      React.createElement(DashboardError, { error: new Error("boom"), reset }),
    );

    expect(screen.getByText(/something went wrong/i)).toBeTruthy();
    expect(screen.getByText(/couldn't load this page/i)).toBeTruthy();

    await userEvent.click(screen.getByTestId("dashboard-error-retry"));
    expect(reset).toHaveBeenCalledOnce();
  });
});

describe("useWorkspaceCreation", () => {
  it("requires the shared provider", async () => {
    const { useWorkspaceCreation } =
      await import("../components/layout/WorkspaceCreationProvider");
    expect(() =>
      renderHook(() =>
        useWorkspaceCreation({
          onSuccess: vi.fn(),
        }),
      ),
    ).toThrow("useWorkspaceCreation requires WorkspaceCreationProvider");
  });

  it("ignores a rejected action after the provider unmounts", async () => {
    let reject: (reason?: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((_, rejectAction) => {
          reject = rejectAction;
        }),
    );
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const onSuccess = vi.fn();
    const hook = renderHook(() => useWorkspaceCreation({ onSuccess }), {
      wrapper: WorkspaceCreationProvider,
    });
    act(() => {
      void hook.result.current.create();
    });
    await waitFor(() => expect(hook.result.current.pending).toBe(true));
    hook.unmount();
    await act(async () => reject(new Error("network unavailable")));
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it("reuses the same create key after an uncertain failure", async () => {
    createWorkspaceActionMock
      .mockRejectedValueOnce(new Error("request timed out"))
      .mockResolvedValueOnce({
        ok: true,
        data: { workspace_id: "ws_recovered", name: "recover-me" },
      });
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const hook = renderHook(
      () => useWorkspaceCreation({ onSuccess: vi.fn() }),
      { wrapper: WorkspaceCreationProvider },
    );

    await act(async () => hook.result.current.create("recover-me"));
    await act(async () => hook.result.current.create("recover-me"));

    const first = createWorkspaceActionMock.mock.calls[0]?.[0] as {
      idempotencyKey: string;
    };
    const retry = createWorkspaceActionMock.mock.calls[1]?.[0] as {
      idempotencyKey: string;
    };
    expect(retry.idempotencyKey).toBe(first.idempotencyKey);
  });

  it("handles completion after the Strict Mode effect replay", async () => {
    let release: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const onSuccess = vi.fn();
    const hook = renderHook(() => useWorkspaceCreation({ onSuccess }), {
      wrapper: ({ children }) =>
        React.createElement(
          React.StrictMode,
          null,
          React.createElement(WorkspaceCreationProvider, null, children),
        ),
    });

    act(() => {
      void hook.result.current.create();
    });
    await act(async () => {
      release({
        ok: true,
        data: { workspace_id: "ws_strict", name: "strict" },
      });
    });

    expect(onSuccess).toHaveBeenCalledWith({
      workspace_id: "ws_strict",
      name: "strict",
    });
  });

  it("upserts a repeated workspace id in the optimistic list", async () => {
    createWorkspaceActionMock
      .mockResolvedValueOnce({
        ok: true,
        data: { workspace_id: "ws_same", name: "first" },
      })
      .mockResolvedValueOnce({
        ok: true,
        data: { workspace_id: "ws_other", name: "other" },
      })
      .mockResolvedValueOnce({
        ok: true,
        data: { workspace_id: "ws_same", name: "renamed" },
      });
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const hook = renderHook(
      () => useWorkspaceCreation({ onSuccess: vi.fn() }),
      { wrapper: WorkspaceCreationProvider },
    );

    await act(async () => hook.result.current.create());
    expect(hook.result.current.createdWorkspaces).toEqual([
      { id: "ws_same", name: "first" },
    ]);
    await act(async () => hook.result.current.create());
    await act(async () => hook.result.current.create());
    expect(hook.result.current.createdWorkspaces).toEqual([
      { id: "ws_same", name: "renamed" },
      { id: "ws_other", name: "other" },
    ]);
  });

  it("releases route settlement when the created workspace is in the pathname", async () => {
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_new", name: "new" },
    });
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const hook = renderHook(
      () =>
        useWorkspaceCreation({
          onSuccess: vi.fn(),
        }),
      { wrapper: WorkspaceCreationProvider },
    );
    await act(async () => hook.result.current.create());
    expect(hook.result.current.locked).toBe(true);
    usePathname.mockReturnValue("/w/ws_new/fleets");
    hook.rerender();
    await waitFor(() => expect(hook.result.current.locked).toBe(false));
  });

  it("keeps late creation settlement locked across an unrelated route change", async () => {
    let release: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    usePathname.mockReturnValue("/settings/billing");
    const firstSuccess = vi.fn();
    const currentSuccess = vi.fn();
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const hook = renderHook(
      ({ onSuccess }) => useWorkspaceCreation({ onSuccess }),
      {
        initialProps: { onSuccess: firstSuccess },
        wrapper: WorkspaceCreationProvider,
      },
    );

    act(() => {
      void hook.result.current.create();
    });
    await waitFor(() => expect(hook.result.current.pending).toBe(true));
    usePathname.mockReturnValue("/settings/models");
    hook.rerender({ onSuccess: currentSuccess });
    await act(async () =>
      release({
        ok: true,
        data: { workspace_id: "ws_late", name: "late" },
      }),
    );

    expect(firstSuccess).not.toHaveBeenCalled();
    expect(currentSuccess).toHaveBeenCalledOnce();
    expect(hook.result.current.locked).toBe(true);
    usePathname.mockReturnValue("/w/ws_late/fleets");
    hook.rerender({ onSuccess: currentSuccess });
    await waitFor(() => expect(hook.result.current.locked).toBe(false));
  });

  it("releases route settlement when refreshed workspace data confirms creation", async () => {
    let knownWorkspaceIds: string[] = [];
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_confirmed", name: "confirmed" },
    });
    const { useWorkspaceCreation, WorkspaceCreationProvider } =
      await import("../components/layout/WorkspaceCreationProvider");
    const hook = renderHook(
      () => useWorkspaceCreation({ onSuccess: vi.fn() }),
      {
        wrapper: ({ children }) => (
          <WorkspaceCreationProvider knownWorkspaceIds={knownWorkspaceIds}>
            {children}
          </WorkspaceCreationProvider>
        ),
      },
    );
    await act(async () => hook.result.current.create());
    expect(hook.result.current.locked).toBe(true);

    knownWorkspaceIds = ["ws_confirmed"];
    hook.rerender();
    await waitFor(() => expect(hook.result.current.locked).toBe(false));
  });
});

describe("NoWorkspaceEmptyState", () => {
  async function renderEmptyState() {
    usePathname.mockReturnValue("/");
    const [{ default: NoWorkspaceEmptyState }, { WorkspaceCreationProvider }] =
      await Promise.all([
        import("../components/layout/NoWorkspaceEmptyState"),
        import("../components/layout/WorkspaceCreationProvider"),
      ]);
    return render(
      React.createElement(
        WorkspaceCreationProvider,
        null,
        React.createElement(NoWorkspaceEmptyState),
      ),
    );
  }

  it("renders the create-first surface and opens the dialog on click", async () => {
    await renderEmptyState();

    expect(screen.getByText(/no workspace yet/i)).toBeTruthy();
    // Closed by default.
    expect(screen.queryByTestId("create-dialog-open")).toBeNull();

    await userEvent.click(screen.getByTestId("create-first-workspace"));
    expect(screen.getByTestId("create-dialog-open")).toBeTruthy();
    await userEvent.click(screen.getByTestId("first-hide"));
    expect(screen.queryByTestId("create-dialog-open")).toBeNull();
  });

  it("navigates after creating the first workspace while attached", async () => {
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_first", name: "first" },
    });
    await renderEmptyState();
    await userEvent.click(screen.getByTestId("create-first-workspace"));
    await userEvent.click(screen.getByTestId("first-submit"));
    await waitFor(() =>
      expect(routerPush).toHaveBeenCalledWith("/w/ws_first/fleets"),
    );
    expect(screen.getByTestId("create-first-workspace").textContent).toBe(
      "Open workspace",
    );
  });

  it("offers navigation recovery without allowing another create", async () => {
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_first", name: "first" },
    });
    await renderEmptyState();
    fireEvent.click(screen.getByTestId("create-first-workspace"));
    fireEvent.click(screen.getByTestId("first-submit"));
    await act(async () => Promise.resolve());
    const recovery = screen.getByTestId("create-first-workspace");
    expect(recovery.textContent).toBe("Open workspace");
    fireEvent.click(recovery);
    expect(routerPush).toHaveBeenLastCalledWith("/w/ws_first/fleets");
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
  });

  it("stays on the empty root when first-workspace creation finishes after Hide", async () => {
    let release: (value: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    await renderEmptyState();
    await userEvent.click(screen.getByTestId("create-first-workspace"));
    await userEvent.click(screen.getByTestId("first-submit"));
    await waitFor(() =>
      expect(
        screen
          .getByTestId("create-first-workspace")
          .getAttribute("aria-disabled"),
      ).toBe("true"),
    );
    await userEvent.click(screen.getByTestId("first-hide"));
    expect(document.activeElement).toBe(
      screen.getByTestId("create-first-workspace"),
    );
    expect(
      screen
        .getByTestId("create-first-workspace")
        .getAttribute("aria-disabled"),
    ).toBe("true");
    await userEvent.click(screen.getByTestId("create-first-workspace"));
    expect(screen.queryByTestId("create-dialog-open")).toBeNull();
    await act(async () => {
      release({ ok: true, data: { workspace_id: "ws_first", name: "first" } });
    });
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(routerPush).not.toHaveBeenCalled();
    const recovery = screen.getByTestId("create-first-workspace");
    expect(recovery.textContent).toBe("Open workspace");
    await userEvent.click(recovery);
    expect(routerPush).toHaveBeenCalledWith("/w/ws_first/fleets");
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
  });

  it("surfaces a rejected first-workspace action after Hide without navigating", async () => {
    let reject: (reason?: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () =>
        new Promise((_, rejectAction) => {
          reject = rejectAction;
        }),
    );
    await renderEmptyState();
    await userEvent.click(screen.getByTestId("create-first-workspace"));
    await userEvent.click(screen.getByTestId("first-submit"));
    await waitFor(() =>
      expect(
        screen
          .getByTestId("create-first-workspace")
          .getAttribute("aria-disabled"),
      ).toBe("true"),
    );
    await userEvent.click(screen.getByTestId("first-hide"));
    vi.useFakeTimers();
    await act(async () => reject(new Error("network unavailable")));
    expect(screen.getByTestId("workspace-toast").textContent).toContain(
      "Couldn't create workspace",
    );
    expect(routerPush).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTimeAsync(2800));
    const toast = screen.getByTestId("workspace-toast");
    expect(toast.textContent).toContain("Couldn't create workspace");
    expect(toast.getAttribute("aria-hidden")).toBe("true");
    await act(async () => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByTestId("workspace-toast").textContent).toBe("");
  });
});
