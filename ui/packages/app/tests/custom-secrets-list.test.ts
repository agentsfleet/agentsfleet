import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const routerRefresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));

// EditCredentialDialog (opened by Replace) talks to the credential actions.
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: vi.fn(),
  deleteCredentialAction: vi.fn(),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return { KeyRoundIcon: make("KeyRoundIcon") };
});

import CustomSecretsList from "@/app/(dashboard)/credentials/components/CustomSecretsList";

const STRIPE = { name: "STRIPE_API_KEY", created_at: Date.UTC(2026, 3, 26, 12) };
const WEBHOOK = { name: "INTERNAL_WEBHOOK", created_at: Date.UTC(2026, 3, 26, 12, 1) };

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => cleanup());

describe("CustomSecretsList (test_custom_secret_create_and_metadata)", () => {
  it("renders a compact empty table row when there are no custom secrets", () => {
    render(React.createElement(CustomSecretsList, { workspaceId: "ws_1", secrets: [] }));
    expect(screen.getByText(/No custom secrets stored/i)).toBeTruthy();
  });

  it("lists each secret with added metadata and a Replace action", () => {
    render(
      React.createElement(CustomSecretsList, { workspaceId: "ws_1", secrets: [STRIPE, WEBHOOK] }),
    );
    expect(screen.getByText("STRIPE_API_KEY")).toBeTruthy();
    expect(screen.getByText("INTERNAL_WEBHOOK")).toBeTruthy();
    expect(screen.getByText("Added")).toBeTruthy();
    expect(screen.queryByText("Stored")).toBeNull();
    expect(screen.getByLabelText("Replace secret STRIPE_API_KEY")).toBeTruthy();
  });

  it("shows the known model-setup reference and 'not referenced' for the rest", () => {
    render(
      React.createElement(CustomSecretsList, {
        workspaceId: "ws_1",
        secrets: [STRIPE, WEBHOOK],
        referencedName: STRIPE.name,
      }),
    );
    // Only the known reference (the active model credential) is surfaced; the
    // other secret reads "not referenced yet" — no usage graph is fabricated.
    const refpill = screen.getByText("model setup");
    expect(refpill).toBeTruthy();
    // The reference renders as a styled "refpill" (rounded, bordered chip) per
    // the design preview — not bare text.
    expect(refpill.className).toContain("rounded-full");
    expect(refpill.className).toContain("border");
    expect(screen.getByText(/not referenced yet/i)).toBeTruthy();
  });

  it("opens the Replace dialog for a secret and closes it", async () => {
    render(React.createElement(CustomSecretsList, { workspaceId: "ws_1", secrets: [STRIPE] }));
    fireEvent.click(screen.getByLabelText("Replace secret STRIPE_API_KEY"));
    await waitFor(() => expect(screen.getByText(/Edit credential .*STRIPE_API_KEY/i)).toBeTruthy());
    // EditCredentialDialog's write-only copy proves Replace, not reveal.
    expect(screen.getByText(/enter the full replacement secret/i)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByText(/Edit credential .*STRIPE_API_KEY/i)).toBeNull());
  });
});
