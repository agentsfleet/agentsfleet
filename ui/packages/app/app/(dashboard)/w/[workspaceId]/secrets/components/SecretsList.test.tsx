import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { formatTimeAbsolute, TooltipProvider } from "@agentsfleet/design-system";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";

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

    // The refusal ends the transition: the row is back, the error is beside it.
    await waitFor(() => expect(screen.getByText("openai")).toBeTruthy());
    expect(screen.getByRole("alertdialog").textContent).toMatch(/vault unavailable/);
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("a confirmed delete closes the dialog and re-reads the server list", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({ ok: true, data: undefined });
    renderList(twoSecrets());

    await confirmDelete("openai");

    await waitFor(() => expect(refreshMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("deleting the last secret shows the empty state at once", async () => {
    deleteSecretActionMock.mockReturnValueOnce(new Promise(() => {}));
    renderList([providerSecret(CREATED_MS)]);

    await confirmDelete("openai");

    await waitFor(() => expect(screen.getByText("No secrets")).toBeTruthy());
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
