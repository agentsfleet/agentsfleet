import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { type Secret } from "@/lib/api/secrets";
import { SECRET_KIND } from "@/lib/api/secrets-types";

// next/navigation + the server action module are the only runtime deps
// SecretsList reaches for; the dynamic edit island renders null while
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
import {
  DELETE_CONNECTION_CONFIRM,
  DELETE_SECRET_CONFIRM,
  DELETE_SECRET_DESCRIPTION,
  deleteConnectionDescription,
} from "../copy";

// A fixed, far-past instant keeps the relative label stable ("… ago", never
// flipping to "in …") and the absolute tooltip deterministic per runtime tz.
const CREATED_AT = "2020-01-15T10:30:00Z";

function providerSecret(created_at: string): Secret {
  return { kind: SECRET_KIND.provider_key, name: "openai", provider: "openai", created_at };
}

// The wrapper stands in for a real ancestor: the root layout mounts the app's
// one TooltipProvider, which the Created cell's relative `Time` reads from.
function renderList(secrets: Secret[], connections?: Record<string, string>) {
  return render(
    React.createElement(SecretsList, { workspaceId: "ws_1", secrets, connections }),
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
    providerSecret(CREATED_AT),
    { kind: SECRET_KIND.provider_key, name: SECOND_SECRET, provider: "anthropic", created_at: CREATED_AT },
  ];
}

/** Click the row's delete, then confirm in the alert dialog. */
async function confirmDelete(name: string) {
  fireEvent.click(screen.getByRole("button", {name: `Manage ${name}`}));
    fireEvent.click(screen.getByRole("button", {name: "Delete secret"}));
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
    renderList([providerSecret(CREATED_AT)]);

    await confirmDelete("openai");

    // The row is gone from view, but "No secrets" is the server's claim to
    // make: the status region is not announced ahead of the answer.
    await waitFor(() => expect(screen.queryByText("openai")).toBeNull());
    expect(screen.queryByText("Keep your credentials in one place")).toBeNull();

    // Settled before the test ends: an async transition left pending would
    // hold React's action scope open for every test that follows.
    settle({ ok: false, error: "vault unavailable", status: 503 });
    await waitFor(() => expect(screen.getByText("openai")).toBeTruthy());
    expect(screen.queryByText("Keep your credentials in one place")).toBeNull();
  });
});

describe("the approved secret rows", () => {
  it("shows type and hidden value without guessing access", () => {
    renderList([providerSecret(CREATED_AT)]);
    expect(screen.getByText("OpenAI key")).toBeTruthy();
    expect(screen.getByText("Hidden")).toBeTruthy();
    expect(screen.getByText("Access details unavailable")).toBeTruthy();
  });

  it("keeps the complete long reference readable and its management action reachable", () => {
    const name = "billing-warehouse-replica-read-only-credentials-for-the-nightly-export";
    renderList([{...providerSecret(CREATED_AT), name}]);
    expect(screen.getByText(name).className).toContain("break-words");
    expect(screen.getByRole("button", {name: `Manage ${name}`})).toBeTruthy();
    expect(screen.queryByRole("button", {name: /Rename secret/})).toBeNull();
  });
});

describe("SecretsList — deleting a connection's secret", () => {
  const GRANT = "github";
  const CONNECTION = "GitHub";

  function grantSecret(name: string): Secret {
    return { kind: SECRET_KIND.custom_secret, name, created_at: CREATED_AT };
  }

  async function openDelete(name: string) {
    fireEvent.click(screen.getByRole("button", {name: `Manage ${name}`}));
    fireEvent.click(screen.getByRole("button", {name: "Delete secret"}));
    return screen.findByRole("alertdialog");
  }

  it("names the connection and disconnects on confirm", async () => {
    deleteSecretActionMock.mockResolvedValueOnce({ ok: true });
    renderList([grantSecret(GRANT)], { [GRANT]: CONNECTION });

    const dialog = await openDelete(GRANT);
    expect(within(dialog).getByText(deleteConnectionDescription(CONNECTION))).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: DELETE_CONNECTION_CONFIRM }));

    await waitFor(() => expect(deleteSecretActionMock).toHaveBeenCalledWith("ws_1", GRANT));
  });

  it("keeps the plain warning for a secret no connection holds", async () => {
    renderList([grantSecret(GRANT)], {});

    const dialog = await openDelete(GRANT);
    expect(within(dialog).getByText(DELETE_SECRET_DESCRIPTION)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: DELETE_SECRET_CONFIRM })).toBeTruthy();
  });

  it("does not read an inherited key as a connection", async () => {
    // `connections.constructor` is Object's own constructor: a plain index
    // would call this secret a live connection.
    renderList([grantSecret("constructor")], { [GRANT]: CONNECTION });

    const dialog = await openDelete("constructor");
    expect(within(dialog).getByText(DELETE_SECRET_DESCRIPTION)).toBeTruthy();
  });
});

it("keeps all workspace secrets visible without making extra requests", () => {
  renderList(twoSecrets());
  expect(screen.getByRole("button", {name: "Manage openai"})).toBeTruthy();
  expect(screen.getByRole("button", {name: "Manage anthropic"})).toBeTruthy();
  expect(refreshMock).not.toHaveBeenCalled();
  expect(deleteSecretActionMock).not.toHaveBeenCalled();
});
