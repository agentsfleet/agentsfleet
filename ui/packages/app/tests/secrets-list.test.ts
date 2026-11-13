import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { SecretActions } from "@/app/(dashboard)/w/[workspaceId]/secrets/components/secret-row-cells";

const routerRefresh = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

const deleteSecretActionMock = vi.fn();
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: vi.fn(),
  deleteSecretAction: deleteSecretActionMock,
}));

vi.mock("@phosphor-icons/react/ssr", async (original) => {
  const actual = await original<Record<string, unknown>>();
  const make = (name: string) => {
    const C = (p: Record<string, unknown>) =>
      React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return { ...actual,
    TrashIcon: make("TrashIcon"),
    // CopyButton (design-system) renders these three.
    CopyIcon: make("CopyIcon"),
    CheckCircleIcon: make("CheckCircleIcon"),
    XIcon: make("XIcon"),
    Loader2Icon: make("Loader2Icon"),
    KeyIcon: make("KeyIcon"),
    MagnifyingGlassIcon: make("MagnifyingGlassIcon"),
    PencilSimpleIcon: make("PencilSimpleIcon"),
    QuestionIcon: make("QuestionIcon"),
    ArrowUpIcon: make("ArrowUpIcon"),
    ArrowDownIcon: make("ArrowDownIcon"),
    ArrowUpDownIcon: make("ArrowUpDownIcon"),
    CaretUpDownIcon: make("CaretUpDownIcon"),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

describe("SecretsList component", () => {
  it("cancels a dialog handoff when its secret row is removed", async () => {
    vi.useFakeTimers();
    try {
      const onEdit = vi.fn();
      const onDelete = vi.fn();
      const rendered = render(React.createElement(SecretActions, {
        secret: {kind: "custom_secret", name: "fly", created_at: "2026-11-01T12:00:00Z"},
        pending: false, protectedFromDelete: false, onEdit, onDelete,
      }));
      fireEvent.click(screen.getByRole("button", {name: "Manage fly"}));
      fireEvent.click(screen.getByRole("button", {name: "Replace value"}));
      rendered.unmount();
      await act(async () => { await vi.runAllTimersAsync(); });
      expect(onEdit).not.toHaveBeenCalled();
      expect(onDelete).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  for (const action of ["Replace value", "Delete secret"]) {
    it(`returns keyboard focus to Manage after cancelling ${action}`, async () => {
      await renderList();
      const user = userEvent.setup();
      const manage = screen.getByRole("button", {name: "Manage fly"});
      await user.click(manage);
      await user.click(screen.getByRole("button", {name: action}));
      const role = action === "Replace value" ? "dialog" : "alertdialog";
      await screen.findByRole(role);
      await user.keyboard("{Escape}");
      await waitFor(() => expect(screen.queryByRole(role)).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(manage));
      expect(deleteSecretActionMock).not.toHaveBeenCalled();
    });
  }

  it("closes secret management without replacing or deleting the value", async () => {
    await renderList();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Done"}));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });
  async function renderList(
    secrets: Array<{ name: string; created_at: string }> = [
      { name: "fly", created_at: new Date(Date.UTC(2026, 3, 26, 12)).toISOString() },
      { name: "slack", created_at: new Date(Date.UTC(2026, 3, 26, 12, 1)).toISOString() },
    ],
    protectedSecretName?: string | null,
  ) {
    const { default: SecretsList } = await import(
      "../app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList"
    );
    const props = {
      workspaceId: "ws_1",
      secrets: secrets.map(secret => ({...secret, kind: "custom_secret"})),
      ...(protectedSecretName === undefined ? {} : { protectedSecretName }),
    };
    // The Created cell now renders a relative <Time>, which mounts a Radix
    // Tooltip — that requires a TooltipProvider ancestor (mounted at the
    // dashboard layout in production, absent in unit renders).
    const element = React.createElement(
      TooltipProvider,
      null,
      React.createElement(SecretsList, props as never),
    );
    const rendered = render(element);
    return {
      ...rendered,
      rerenderList(nextProtectedSecretName: string | null) {
        rendered.rerender(
          React.createElement(
            TooltipProvider,
            null,
            React.createElement(SecretsList, {
              workspaceId: "ws_1",
              secrets: secrets.map(secret => ({...secret, kind: "custom_secret"})),
              protectedSecretName: nextProtectedSecretName,
            } as never),
          ),
        );
      },
    };
  }

  it("disables delete for the secret used by model setup", async () => {
    const user = userEvent.setup();
    await renderList(undefined, "fly");

    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    const protectedDelete = screen.getByRole("button", {name: "Delete secret"});
    expect((protectedDelete as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/Choose another model before deleting it/)).toBeTruthy();
    await user.click(protectedDelete);

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("does not confirm delete when the secret becomes protected while the dialog is open", async () => {
    const user = userEvent.setup();
    const rendered = await renderList();

    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    rendered.rerenderList("fly");
    await user.click(screen.getByRole("button", { name: /^delete$/i }));

    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("renders the empty-state message when no secrets", async () => {
    await renderList([]);
    expect(screen.getByText(/^Keep your credentials in one place$/i)).toBeTruthy();
  });

  it("shows the approved columns without implying the API returned secret values or fleet access", async () => {
    await renderList();
    for (const name of ["Name", "Type", "Value", "Available to"]) expect(screen.getByRole("columnheader", {name})).toBeTruthy();
    expect(screen.getAllByText("Hidden")).toHaveLength(2);
    expect(screen.getAllByText("Access details unavailable")).toHaveLength(2);
    expect(screen.queryByRole("columnheader", {name: "Created"})).toBeNull();
  });

  it("test_secret_write_only_masked: stored secret is masked (suffix only), Replace present, never re-revealed", async () => {
    const user = userEvent.setup();
    // The vault never returns plaintext, so the list renders only the name +
    // creation date — the secret value never appears in the DOM.
    await renderList([{ name: "openai-key", created_at: new Date(Date.UTC(2026, 3, 26, 12)).toISOString() }]);
    expect(screen.getByText("openai-key")).toBeTruthy();
    expect(screen.queryByText("Encrypted and hidden.")).toBeNull();
    // No plaintext secret is rendered anywhere.
    expect(document.body.textContent).not.toMatch(/sk-[a-z0-9]/i);

    // The edit affordance is a Replace/Rotate flow, never a reveal: opening it
    // asks the user to re-enter the secret rather than displaying the stored one.
    await user.click(screen.getByRole("button", {name: "Manage openai-key"}));
    await user.click(screen.getByRole("button", {name: "Replace value"}));
    await waitFor(() => expect(screen.getByRole("dialog", {name: "Replace secret"})).toBeTruthy());
    expect(screen.getByText("“openai-key”")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^replace value$/i })).toBeTruthy();
    expect(screen.getByText(/re-enter every field you want to keep/i)).toBeTruthy();
    // There is no "reveal"/"show" control that would expose the stored value.
    expect(screen.queryByRole("button", { name: /reveal|show secret/i })).toBeNull();
  });

  it("happy path: click delete then confirm calls delete and refreshes", async () => {
    deleteSecretActionMock.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(deleteSecretActionMock).toHaveBeenCalledWith("ws_1", "fly"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("delete failure surfaces errorMessage and keeps the dialog open", async () => {
    deleteSecretActionMock.mockResolvedValue({ ok: false, error: "network down" });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(deleteSecretActionMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/network down/),
    );
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
    // A transport failure carries no status, so the outcome is unknown and
    // the list re-reads the server once to learn whether the row survived.
    expect(routerRefresh).toHaveBeenCalledTimes(1);
  });

  it("unauthenticated action result surfaces Not authenticated", async () => {
    deleteSecretActionMock.mockResolvedValue({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("cancel on dialog clears target without invoking delete", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage slack"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("confirm on an empty-named secret is a no-op", async () => {
    const user = userEvent.setup();
    await renderList([{ name: "", created_at: new Date(Date.UTC(2026, 3, 26, 12, 2)).toISOString() }]);
    await user.click(screen.getByRole("button", {name: "Manage"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^delete$/i })).toBeTruthy());
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("clicking edit opens the rotate-only edit dialog, and Cancel closes it", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Replace value"}));
    await waitFor(() => expect(screen.getByRole("dialog", {name: "Replace secret"})).toBeTruthy());
    expect(screen.getByText("“fly”")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^replace value$/i })).toBeTruthy();
    // Editing keeps the reference name fixed.
    expect(screen.queryByRole("button", { name: /rename/i })).toBeNull();
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Replace secret" })).toBeNull());
  });


  it("error from a previous attempt clears when reopening another secret", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({ ok: false, error: "boom" });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByRole("button", {name: "Manage fly"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByRole("button", {name: "Manage slack"}));
    await user.click(screen.getByRole("button", {name: "Delete secret"}));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
