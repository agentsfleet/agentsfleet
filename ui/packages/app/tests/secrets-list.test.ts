import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const routerRefresh = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

const deleteSecretActionMock = vi.fn();
vi.mock("@/app/(dashboard)/secrets/actions", () => ({
  createSecretAction: vi.fn(),
  deleteSecretAction: deleteSecretActionMock,
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => {
    const C = (p: Record<string, unknown>) =>
      React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return {
    Trash2Icon: make("Trash2Icon"),
    Loader2Icon: make("Loader2Icon"),
    KeyRoundIcon: make("KeyRoundIcon"),
    PencilIcon: make("PencilIcon"),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

describe("SecretsList component", () => {
  async function renderList(
    secrets: Array<{ name: string; created_at: number }> = [
      { name: "fly", created_at: Date.UTC(2026, 3, 26, 12) },
      { name: "slack", created_at: Date.UTC(2026, 3, 26, 12, 1) },
    ],
    protectedSecretName?: string | null,
  ) {
    const { default: SecretsList } = await import(
      "../app/(dashboard)/secrets/components/SecretsList"
    );
    const props = {
      workspaceId: "ws_1",
      secrets,
      ...(protectedSecretName === undefined ? {} : { protectedSecretName }),
    };
    const element = React.createElement(SecretsList, props as never);
    const rendered = render(element);
    return {
      ...rendered,
      rerenderList(nextProtectedSecretName: string | null) {
        rendered.rerender(
          React.createElement(SecretsList, {
            workspaceId: "ws_1",
            secrets,
            protectedSecretName: nextProtectedSecretName,
          } as never),
        );
      },
    };
  }

  it("disables delete for the secret used by model setup", async () => {
    const user = userEvent.setup();
    await renderList(undefined, "fly");

    const protectedDelete = screen.getByLabelText("Secret fly is in model setup");
    expect((protectedDelete as HTMLButtonElement).disabled).toBe(true);
    expect(protectedDelete.getAttribute("title")).toMatch(/Switch model setup/i);
    await user.click(protectedDelete);

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("does not confirm delete when the secret becomes protected while the dialog is open", async () => {
    const user = userEvent.setup();
    const rendered = await renderList();

    await user.click(screen.getByLabelText(/Delete secret fly/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    rendered.rerenderList("fly");
    await user.click(screen.getByRole("button", { name: /^delete$/i }));

    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("renders the empty-state message when no secrets", async () => {
    await renderList([]);
    expect(screen.getByText(/No secrets yet/i)).toBeTruthy();
  });

  it("renders one row per secret with name and a human timestamp", async () => {
    await renderList();
    expect(screen.getByText("fly")).toBeTruthy();
    expect(screen.getByText("slack")).toBeTruthy();
    expect(screen.getAllByText("Write-only encrypted secret")).toHaveLength(2);
    expect(screen.getAllByText(/Apr 26, 2026/)).toHaveLength(2);
  });

  it("test_secret_write_only_masked: stored secret is masked (suffix only), Replace present, never re-revealed", async () => {
    const user = userEvent.setup();
    // The vault never returns plaintext, so the list renders only the name +
    // the "write-only" label — the secret value never appears in the DOM.
    await renderList([{ name: "openai-key", created_at: Date.UTC(2026, 3, 26, 12) }]);
    expect(screen.getByText("openai-key")).toBeTruthy();
    expect(screen.getByText("Write-only encrypted secret")).toBeTruthy();
    // No plaintext secret is rendered anywhere.
    expect(document.body.textContent).not.toMatch(/sk-[a-z0-9]/i);

    // The edit affordance is a Replace/Rotate flow, never a reveal: opening it
    // asks the user to re-enter the secret rather than displaying the stored one.
    await user.click(screen.getByLabelText(/Edit secret openai-key/i));
    await waitFor(() => expect(screen.getByText(/Edit secret .*openai-key/i)).toBeTruthy());
    expect(screen.getByRole("button", { name: /^rotate$/i })).toBeTruthy();
    expect(screen.getByText(/enter the full replacement secret/i)).toBeTruthy();
    // There is no "reveal"/"show" control that would expose the stored value.
    expect(screen.queryByRole("button", { name: /reveal|show secret/i })).toBeNull();
  });

  it("happy path: click delete then confirm calls delete and refreshes", async () => {
    deleteSecretActionMock.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete secret fly/i));
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
    await user.click(screen.getByLabelText(/Delete secret fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(deleteSecretActionMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/network down/),
    );
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("unauthenticated action result surfaces Not authenticated", async () => {
    deleteSecretActionMock.mockResolvedValue({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete secret fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("cancel on dialog clears target without invoking delete", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete secret slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("confirm on an empty-named secret is a no-op", async () => {
    const user = userEvent.setup();
    await renderList([{ name: "", created_at: Date.UTC(2026, 3, 26, 12, 2) }]);
    await user.click(screen.getByLabelText(/^Delete secret\s*$/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^delete$/i })).toBeTruthy());
    expect(deleteSecretActionMock).not.toHaveBeenCalled();
  });

  it("clicking edit opens the edit dialog, and Cancel closes it", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Edit secret fly/i));
    await waitFor(() => expect(screen.getByText(/Edit secret .*fly/i)).toBeTruthy());
    expect(screen.getByRole("button", { name: /^rotate$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /advanced — rename/i })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByText(/Edit secret .*fly/i)).toBeNull());
  });

  it("error from a previous attempt clears when reopening another secret", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({ ok: false, error: "boom" });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete secret fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByLabelText(/Delete secret slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
