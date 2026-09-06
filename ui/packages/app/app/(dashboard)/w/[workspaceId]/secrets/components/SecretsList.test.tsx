import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { formatTimeAbsolute, TooltipProvider } from "@agentsfleet/design-system";
import { type Secret } from "@/lib/api/secrets";
import { SECRET_KIND } from "@/lib/api/secrets-types";

// next/navigation + the server action module are the only runtime deps
// SecretsList reaches for; the dynamic edit/rename islands render null while
// closed (open is parent-driven), so no stub is needed for them.
const { refreshMock, deleteSecretActionMock } = vi.hoisted(() => ({
  refreshMock: vi.fn(),
  deleteSecretActionMock: vi.fn(),
}));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock, push: vi.fn() }),
}));
vi.mock("../actions", () => ({ deleteSecretAction: deleteSecretActionMock }));

import SecretsList from "./SecretsList";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SOURCE_PATH = path.join(__dirname, "SecretsList.tsx");

// A fixed, far-past instant keeps the relative label stable ("… ago", never
// flipping to "in …") and the absolute tooltip deterministic per runtime tz.
const CREATED_MS = Date.UTC(2020, 0, 15, 10, 30, 0);

function providerSecret(created_at: number): Secret {
  return { kind: SECRET_KIND.provider_key, name: "openai", provider: "openai", created_at };
}

// The wrapper stands in for a real ancestor: the root layout mounts the app's
// one TooltipProvider, which the Created cell's relative `Time` reads from.
function renderList(secrets: Secret[]) {
  return render(
    React.createElement(SecretsList, { workspaceId: "ws_1", secrets }),
    { wrapper: TooltipProvider },
  );
}

afterEach(() => {
  cleanup();
  refreshMock.mockReset();
  deleteSecretActionMock.mockReset();
});

const SECOND_SECRET = "anthropic";

function twoSecrets(): Secret[] {
  return [
    providerSecret(CREATED_MS),
    { kind: SECRET_KIND.provider_key, name: SECOND_SECRET, provider: "anthropic", created_at: CREATED_MS },
  ];
}

/** Click the row's delete, then confirm in the alert dialog. */
async function confirmDelete(name: string) {
  fireEvent.click(screen.getByRole("button", { name: `Delete secret ${name}` }));
  const dialog = await screen.findByRole("alertdialog");
  fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
}

describe("SecretsList — optimistic delete", () => {
  it("a secret row leaves on confirm and returns on failure", async () => {
    let settle: (result: { ok: false; error: string; status: number }) => void = () => {};
    deleteSecretActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    renderList(twoSecrets());
    expect(screen.getByText("openai")).toBeTruthy();

    await confirmDelete("openai");

    // Gone before the server has answered.
    await waitFor(() => expect(screen.queryByText("openai")).toBeNull());
    expect(screen.getByText(SECOND_SECRET)).toBeTruthy();
    expect(deleteSecretActionMock).toHaveBeenCalledWith("ws_1", "openai");

    settle({ ok: false, error: "vault unavailable", status: 503 });

    // The failure ends the transition: the row is back, the error is beside
    // it, and — a 503 leaves the outcome unknown, the vault may have deleted
    // the row before its gateway gave up — the list re-reads the server.
    await waitFor(() => expect(screen.getByText("openai")).toBeTruthy());
    expect(screen.getByRole("alertdialog").textContent).toMatch(/vault unavailable/);
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("a refusal the server made restores the row without a read", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({
      ok: false,
      error: "the fleet still references it",
      status: 409,
    });
    renderList(twoSecrets());

    await confirmDelete("openai");

    // A 409 is the server's word that nothing changed: the transition's end
    // restores the row, and a read would only restate what is already shown.
    await waitFor(() =>
      expect(screen.getByRole("alertdialog").textContent).toMatch(/still references it/),
    );
    await waitFor(() => expect(screen.getByText("openai")).toBeTruthy());
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("the dialog holds its buttons disabled until the delete settles", async () => {
    let settle: (result: { ok: true; data: undefined }) => void = () => {};
    deleteSecretActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    renderList(twoSecrets());

    await confirmDelete("openai");

    // In flight: both buttons disabled, the confirm reads Working…, and a
    // second click sends nothing.
    const dialog = screen.getByRole("alertdialog");
    const working = await within(dialog).findByRole("button", { name: "Working…" });
    expect(working.hasAttribute("disabled")).toBe(true);
    expect(within(dialog).getByRole("button", { name: "Cancel" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(working);
    expect(deleteSecretActionMock).toHaveBeenCalledTimes(1);

    settle({ ok: true, data: undefined });

    await waitFor(() => expect(refreshMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("a confirmed delete closes the dialog and re-reads the server list", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({ ok: true, data: undefined });
    renderList(twoSecrets());

    await confirmDelete("openai");

    await waitFor(() => expect(refreshMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("deleting the last secret keeps the table shell until the server confirms", async () => {
    let settle: (result: { ok: false; error: string; status: number }) => void = () => {};
    deleteSecretActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    renderList([providerSecret(CREATED_MS)]);

    await confirmDelete("openai");

    // The row is gone from view, but "No secrets" is the server's claim to
    // make: the status region is not announced ahead of the answer.
    await waitFor(() => expect(screen.queryByText("openai")).toBeNull());
    expect(screen.queryByText("No secrets")).toBeNull();

    // Settled before the test ends: an async transition left pending would
    // hold React's action scope open for every test that follows.
    settle({ ok: false, error: "vault unavailable", status: 503 });
    await waitFor(() => expect(screen.getByText("openai")).toBeTruthy());
    expect(screen.queryByText("No secrets")).toBeNull();
  });
});

describe("SecretsList Created cell", () => {
  it("test_secrets_created_relative", async () => {
    const { container } = renderList([providerSecret(CREATED_MS)]);

    // The Created cell renders a <time> whose datetime is the ISO instant and
    // whose visible text is the relative "… ago" label.
    const timeEl = container.querySelector("time");
    expect(timeEl).not.toBeNull();
    expect(timeEl!.getAttribute("datetime")).toBe(new Date(CREATED_MS).toISOString());
    expect(timeEl!.textContent).toMatch(/ago$/);

    // Focus opens the Radix tooltip, which carries the absolute timestamp.
    const absolute = formatTimeAbsolute(new Date(CREATED_MS));
    fireEvent.focus(timeEl!);
    const tips = await screen.findAllByText(absolute);
    expect(tips.length).toBeGreaterThan(0);
  });

  it("test_secretslist_no_bespoke_formatter", () => {
    const source = readFileSync(SOURCE_PATH, "utf8");
    expect(source).not.toContain("DATE_FORMATTER");
    expect(source).not.toContain("formatCreatedAt");
  });
});
