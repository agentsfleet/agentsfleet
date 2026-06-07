import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { createCredentialActionMock, routerRefresh } = vi.hoisted(() => ({
  createCredentialActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: createCredentialActionMock,
}));

import InlineProviderKeyCreate from "@/app/(dashboard)/settings/models/components/InlineProviderKeyCreate";

const WORKSPACE_ID = "ws_inline_test";

beforeEach(() => {
  createCredentialActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

function fillKeyFields(provider: string, apiKey: string, model: string) {
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: provider } });
  fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: apiKey } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: model } });
}

describe("InlineProviderKeyCreate", () => {
  it("auto-names the credential after the provider until the name is edited", () => {
    render(React.createElement(InlineProviderKeyCreate, { workspaceId: WORKSPACE_ID, onCreated: vi.fn() }));

    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    expect((screen.getByLabelText(/credential name/i) as HTMLInputElement).value).toBe("anthropic");

    // Once the user types their own name it stops tracking the provider.
    fireEvent.change(screen.getByLabelText(/credential name/i), { target: { value: "anthropic-prod" } });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic-eu" } });
    expect((screen.getByLabelText(/credential name/i) as HTMLInputElement).value).toBe("anthropic-prod");
  });

  it("submits {provider, api_key, model} under the auto-name and selects the new credential", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    const onCreated = vi.fn();
    render(React.createElement(InlineProviderKeyCreate, { workspaceId: WORKSPACE_ID, onCreated }));

    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, {
      name: "anthropic",
      data: { provider: "anthropic", api_key: "sk-ant-secret", model: "claude-sonnet-4-6" },
    });
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith("anthropic"));
  });

  it("surfaces a duplicate-name error and does not select the credential", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "a credential with that name already exists",
      errorCode: "UZ-CRED-409",
      status: 409,
    });
    const onCreated = vi.fn();
    render(React.createElement(InlineProviderKeyCreate, { workspaceId: WORKSPACE_ID, onCreated }));

    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
    expect(onCreated).not.toHaveBeenCalled();
  });
});
