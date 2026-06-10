import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";

// ── Shared mocks ───────────────────────────────────────────────────────────

const createApiKeyActionMock = vi.fn();
const captureProductEventMock = vi.fn();

vi.mock("@/app/(dashboard)/settings/api-keys/actions", () => ({
  listApiKeysAction: vi.fn(),
  createApiKeyAction: createApiKeyActionMock,
  revokeApiKeyAction: vi.fn(),
  deleteApiKeyAction: vi.fn(),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

// happy-dom ships a real (no-op) navigator.clipboard.writeText; defining a fresh
// object on the instance does not shadow it, so spy on the live method instead.
// Guard for environments where the API is absent before spying.
function stubClipboardWriteText() {
  if (!navigator.clipboard) {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async () => {} },
      configurable: true,
    });
  }
  return vi.spyOn(navigator.clipboard, "writeText");
}

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CreateApiKeyDialog component", () => {
  async function openDialog(onCreated = vi.fn()) {
    const { default: CreateApiKeyDialog } = await import(
      "../app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog"
    );
    const user = userEvent.setup({ delay: null });
    render(React.createElement(CreateApiKeyDialog, { onCreated } as never));
    await user.click(screen.getByRole("button", { name: /new api key/i }));
    await waitFor(() => expect(screen.getByLabelText(/^name$/i)).toBeTruthy());
    return { user, onCreated };
  }

  it("client-side rejects an invalid key name and never calls the action", async () => {
    const { user } = await openDialog();
    await user.type(screen.getByLabelText(/^name$/i), "bad name!");
    await user.click(screen.getByRole("button", { name: /create key/i }));
    await waitFor(() => expect(screen.getByText(/letters, digits, hyphen, underscore/i)).toBeTruthy());
    expect(createApiKeyActionMock).not.toHaveBeenCalled();
  });

  it("happy path: reveals the raw key exactly once, then discards it on close", async () => {
    createApiKeyActionMock.mockResolvedValue({
      ok: true,
      data: { id: "k", key_name: "ci-runner", key: "zmb_t_deadbeef", created_at: 1 },
    });
    const { user, onCreated } = await openDialog();
    await user.type(screen.getByLabelText(/^name$/i), "ci-runner");
    await user.click(screen.getByRole("button", { name: /create key/i }));

    const field = await screen.findByLabelText(/API key value/i);
    expect((field as HTMLInputElement).value).toBe("zmb_t_deadbeef");

    await user.click(screen.getByRole("button", { name: /stored it/i }));
    await waitFor(() => expect(screen.queryByDisplayValue("zmb_t_deadbeef")).toBeNull());
    expect(onCreated).toHaveBeenCalled();

    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.api_key_minted, { api_key_id: "k" });
    // The one-time raw key must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("zmb_t_deadbeef");
  });

  it("name collision keeps the dialog open and reveals no key", async () => {
    createApiKeyActionMock.mockResolvedValue({ ok: false, error: "name taken", errorCode: "UZ-APIKEY-005" });
    const { user, onCreated } = await openDialog();
    await user.type(screen.getByLabelText(/^name$/i), "ci-runner");
    await user.click(screen.getByRole("button", { name: /create key/i }));
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());
    expect(screen.queryByLabelText(/API key value/i)).toBeNull();
    expect(onCreated).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  async function reachReveal(user: ReturnType<typeof userEvent.setup>) {
    createApiKeyActionMock.mockResolvedValue({
      ok: true,
      data: { id: "k", key_name: "ci-runner", key: "zmb_t_deadbeef", created_at: 1 },
    });
    await user.type(screen.getByLabelText(/^name$/i), "ci-runner");
    await user.click(screen.getByRole("button", { name: /create key/i }));
    await screen.findByLabelText(/API key value/i);
  }

  it("copies the raw key to the clipboard on demand", async () => {
    const writeText = stubClipboardWriteText().mockResolvedValue(undefined);
    const { user } = await openDialog();
    await reachReveal(user);
    await user.click(screen.getByRole("button", { name: /copy to clipboard/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("zmb_t_deadbeef"));
    expect(screen.getByRole("button", { name: /^copied$/i })).toBeTruthy();
  });

  it("falls back to manual selection when the clipboard API is blocked", async () => {
    stubClipboardWriteText().mockRejectedValue(new Error("blocked"));
    const { user } = await openDialog();
    await reachReveal(user);
    await user.click(screen.getByRole("button", { name: /copy to clipboard/i }));
    await waitFor(() => expect(screen.getByText(/copy failed — select the value/i)).toBeTruthy());
    // The reveal stays intact so the user can still grab the value manually.
    expect((screen.getByLabelText(/API key value/i) as HTMLInputElement).value).toBe("zmb_t_deadbeef");
  });

  it("selects the whole key value on focus so it can be copied manually", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    const input = screen.getByLabelText(/API key value/i) as HTMLInputElement;
    const select = vi.spyOn(input, "select");
    fireEvent.focus(input);
    expect(select).toHaveBeenCalled();
  });

  it("keeps the dialog open on Escape while the one-time key is revealed", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    await user.keyboard("{Escape}");
    // The reveal must survive the Escape — the key is shown exactly once.
    expect(screen.getByLabelText(/API key value/i)).toBeTruthy();
  });

  it("keeps the dialog open on an outside click while the key is revealed", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    // Radix fires onInteractOutside on a pointerdown outside the content; the
    // overlay-lock must preventDefault so the one-time key isn't lost.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    expect(screen.getByLabelText(/API key value/i)).toBeTruthy();
  });

  it("closes on Escape before a key is minted (no overlay lock yet)", async () => {
    const { user } = await openDialog();
    await screen.findByLabelText(/^name$/i);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByLabelText(/^name$/i)).toBeNull());
  });

  it("closes on an outside click before a key is minted (no overlay lock yet)", async () => {
    await openDialog();
    await screen.findByLabelText(/^name$/i);
    // created is still null → onInteractOutside does NOT preventDefault → closes.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    await waitFor(() => expect(screen.queryByLabelText(/^name$/i)).toBeNull());
  });
});
