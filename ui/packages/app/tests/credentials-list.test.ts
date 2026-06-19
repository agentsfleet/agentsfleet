import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const routerRefresh = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

const deleteCredentialActionMock = vi.fn();
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: vi.fn(),
  deleteCredentialAction: deleteCredentialActionMock,
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

describe("CredentialsList component", () => {
  async function renderList(
    credentials: Array<{ name: string; created_at: number }> = [
      { name: "fly", created_at: Date.UTC(2026, 3, 26, 12) },
      { name: "slack", created_at: Date.UTC(2026, 3, 26, 12, 1) },
    ],
    protectedCredentialName?: string | null,
  ) {
    const { default: CredentialsList } = await import(
      "../app/(dashboard)/credentials/components/CredentialsList"
    );
    const props = {
      workspaceId: "ws_1",
      credentials,
      ...(protectedCredentialName === undefined ? {} : { protectedCredentialName }),
    };
    const element = React.createElement(CredentialsList, props as never);
    const rendered = render(element);
    return {
      ...rendered,
      rerenderList(nextProtectedCredentialName: string | null) {
        rendered.rerender(
          React.createElement(CredentialsList, {
            workspaceId: "ws_1",
            credentials,
            protectedCredentialName: nextProtectedCredentialName,
          } as never),
        );
      },
    };
  }

  it("disables delete for the credential used by model setup", async () => {
    const user = userEvent.setup();
    await renderList(undefined, "fly");

    const protectedDelete = screen.getByLabelText("Credential fly is in model setup");
    expect((protectedDelete as HTMLButtonElement).disabled).toBe(true);
    expect(protectedDelete.getAttribute("title")).toMatch(/Switch model setup/i);
    await user.click(protectedDelete);

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
  });

  it("does not confirm delete when the credential becomes protected while the dialog is open", async () => {
    const user = userEvent.setup();
    const rendered = await renderList();

    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    rendered.rerenderList("fly");
    await user.click(screen.getByRole("button", { name: /^delete$/i }));

    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
  });

  it("renders the empty-state message when no credentials", async () => {
    await renderList([]);
    expect(screen.getByText(/No credentials yet/i)).toBeTruthy();
  });

  it("renders one row per credential with name and a human timestamp", async () => {
    await renderList();
    expect(screen.getByText("fly")).toBeTruthy();
    expect(screen.getByText("slack")).toBeTruthy();
    expect(screen.getAllByText("Write-only encrypted secret")).toHaveLength(2);
    expect(screen.getAllByText(/Apr 26, 2026/)).toHaveLength(2);
  });

  it("happy path: click delete then confirm calls delete and refreshes", async () => {
    deleteCredentialActionMock.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(deleteCredentialActionMock).toHaveBeenCalledWith("ws_1", "fly"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("delete failure surfaces errorMessage and keeps the dialog open", async () => {
    deleteCredentialActionMock.mockResolvedValue({ ok: false, error: "network down" });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(deleteCredentialActionMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/network down/),
    );
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("unauthenticated action result surfaces Not authenticated", async () => {
    deleteCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("cancel on dialog clears target without invoking delete", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
  });

  it("confirm on an empty-named credential is a no-op", async () => {
    const user = userEvent.setup();
    await renderList([{ name: "", created_at: Date.UTC(2026, 3, 26, 12, 2) }]);
    await user.click(screen.getByLabelText(/^Delete credential\s*$/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^delete$/i })).toBeTruthy());
    expect(deleteCredentialActionMock).not.toHaveBeenCalled();
  });

  it("clicking edit opens the edit dialog, and Cancel closes it", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Edit credential fly/i));
    await waitFor(() => expect(screen.getByText(/Edit credential .*fly/i)).toBeTruthy());
    expect(screen.getByRole("button", { name: /^rotate$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /advanced — rename/i })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByText(/Edit credential .*fly/i)).toBeNull());
  });

  it("error from a previous attempt clears when reopening another credential", async () => {
    deleteCredentialActionMock.mockResolvedValueOnce({ ok: false, error: "boom" });
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByLabelText(/Delete credential slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
