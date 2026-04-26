import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// ── Shared mocks ───────────────────────────────────────────────────────────

const routerRefresh = vi.fn();
const getTokenFn = vi.fn().mockResolvedValue("token_abc");

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: getTokenFn }),
  useUser: () => ({ isLoaded: true, isSignedIn: true, user: null }),
  ClerkProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children),
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  SignIn: () => React.createElement("div", { "data-sign-in": "1" }),
  SignUp: () => React.createElement("div", { "data-sign-up": "1" }),
}));

const listCredentialsMock = vi.fn();
const createCredentialMock = vi.fn();
const deleteCredentialMock = vi.fn();
vi.mock("@/lib/api/credentials", () => ({
  listCredentials: listCredentialsMock,
  createCredential: createCredentialMock,
  deleteCredential: deleteCredentialMock,
}));

// Use the real ConfirmDialog (for errorMessage rendering) and lucide stubs;
// stub only the form primitives that pull radix client-only providers we
// don't need at unit level.
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
  };
});

beforeEach(() => {
  vi.clearAllMocks();
  getTokenFn.mockResolvedValue("token_abc");
});

afterEach(() => cleanup());

// ── CredentialsList ────────────────────────────────────────────────────────

describe("CredentialsList component", () => {
  async function renderList(
    credentials: Array<{ name: string; created_at: string }> = [
      { name: "fly", created_at: "2026-04-26T00:00:00Z" },
      { name: "slack", created_at: "2026-04-26T00:00:01Z" },
    ],
  ) {
    const { default: CredentialsList } = await import(
      "../app/(dashboard)/credentials/components/CredentialsList"
    );
    render(
      React.createElement(CredentialsList, {
        workspaceId: "ws_1",
        credentials,
      } as never),
    );
  }

  it("renders the empty-state message when no credentials", async () => {
    await renderList([]);
    expect(screen.getByText(/No credentials stored yet/i)).toBeTruthy();
  });

  it("renders one row per credential with name + created_at", async () => {
    await renderList();
    expect(screen.getByText("fly")).toBeTruthy();
    expect(screen.getByText("slack")).toBeTruthy();
    expect(screen.getByText("2026-04-26T00:00:00Z")).toBeTruthy();
  });

  it("happy path: click delete → confirm → deleteCredential called → router refresh", async () => {
    deleteCredentialMock.mockResolvedValue(undefined);
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(deleteCredentialMock).toHaveBeenCalledWith("ws_1", "fly", "token_abc"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("delete failure surfaces errorMessage, dialog stays open", async () => {
    deleteCredentialMock.mockRejectedValue(new Error("network down"));
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(deleteCredentialMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/network down/),
    );
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("missing token surfaces Not authenticated and does not call deleteCredential", async () => {
    getTokenFn.mockResolvedValue(null);
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
    expect(deleteCredentialMock).not.toHaveBeenCalled();
  });

  it("cancel on dialog clears target without invoking delete", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteCredentialMock).not.toHaveBeenCalled();
  });

  it("error from a previous attempt clears when reopening for another credential", async () => {
    deleteCredentialMock.mockRejectedValueOnce(new Error("boom"));
    const user = userEvent.setup();
    await renderList();
    await user.click(screen.getByLabelText(/Delete credential fly/i));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
    // Cancel the failed dialog, then click another credential — the prior
    // error must not leak into the new dialog.
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByLabelText(/Delete credential slack/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

// ── AddCredentialForm ──────────────────────────────────────────────────────

describe("AddCredentialForm component", () => {
  async function renderForm() {
    const { default: AddCredentialForm } = await import(
      "../app/(dashboard)/credentials/components/AddCredentialForm"
    );
    render(React.createElement(AddCredentialForm, { workspaceId: "ws_1" } as never));
  }

  it("renders name + data inputs + submit button", async () => {
    await renderForm();
    expect(screen.getByLabelText(/^name$/i)).toBeTruthy();
    expect(screen.getByLabelText(/data \(json object\)/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /store credential/i })).toBeTruthy();
  });

  it("submit with empty fields shows zod required errors", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() => {
      expect(screen.getByText(/Credential name is required/i)).toBeTruthy();
      expect(screen.getByText(/Credential data is required/i)).toBeTruthy();
    });
    expect(createCredentialMock).not.toHaveBeenCalled();
  });

  // `userEvent.type` interprets `{` and `[` as keyboard descriptors, so use
  // fireEvent.change to set the textarea value verbatim for these cases.

  it("submit with invalid JSON shows Invalid JSON error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "{not json" },
    });
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(screen.getByText(/Invalid JSON:/i)).toBeTruthy(),
    );
    expect(createCredentialMock).not.toHaveBeenCalled();
  });

  it("submit with array JSON rejects (must be object)", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "[1,2,3]" },
    });
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(screen.getByText(/Data must be a JSON object/i)).toBeTruthy(),
    );
    expect(createCredentialMock).not.toHaveBeenCalled();
  });

  it("submit with empty object rejects (must have one field)", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    fireEvent.change(screen.getByLabelText(/data \(json object\)/i), {
      target: { value: "{}" },
    });
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(screen.getByText(/Object must have at least one field/i)).toBeTruthy(),
    );
    expect(createCredentialMock).not.toHaveBeenCalled();
  });

  it("happy path: createCredential called with parsed data, then router refresh", async () => {
    createCredentialMock.mockResolvedValue({ name: "fly" });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"api.machines.dev","api_token":"T"}',
    );
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(createCredentialMock).toHaveBeenCalledWith(
        "ws_1",
        { name: "fly", data: { host: "api.machines.dev", api_token: "T" } },
        "token_abc",
      ),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("API error renders apiError below the form", async () => {
    createCredentialMock.mockRejectedValue(
      Object.assign(new Error("data too large"), { status: 400 }),
    );
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"x"}',
    );
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(screen.getByText(/data too large/i)).toBeTruthy(),
    );
  });

  it("missing token surfaces Not authenticated", async () => {
    getTokenFn.mockResolvedValue(null);
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^name$/i), "fly");
    await user.type(
      screen.getByLabelText(/data \(json object\)/i),
      '{{"host":"x"}',
    );
    await user.click(screen.getByRole("button", { name: /store credential/i }));
    await waitFor(() =>
      expect(screen.getByText(/Not authenticated/i)).toBeTruthy(),
    );
    expect(createCredentialMock).not.toHaveBeenCalled();
  });
});
