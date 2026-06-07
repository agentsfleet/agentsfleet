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
import type { ModelCap } from "@/lib/api/model_caps";

const WORKSPACE_ID = "ws_inline_test";
const MODELS: ModelCap[] = [
  { id: "claude-sonnet-4-6", provider: "anthropic", context_cap_tokens: 256000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
  { id: "kimi-k2.6", provider: "moonshot", context_cap_tokens: 256000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
];

beforeEach(() => {
  createCredentialActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

function renderForm(extra: { catalogue?: ModelCap[]; onCreated?: (name: string) => void } = {}) {
  return render(
    React.createElement(InlineProviderKeyCreate, {
      workspaceId: WORKSPACE_ID,
      catalogue: MODELS,
      onCreated: vi.fn(),
      ...extra,
    }),
  );
}

function fillKeyFields(provider: string, apiKey: string, model: string) {
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: provider } });
  fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: apiKey } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: model } });
}

describe("InlineProviderKeyCreate", () => {
  it("paste-fills the provider and scopes the model picker to the detected provider", () => {
    renderForm();
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-ant-secret123" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("anthropic");
    // The model field becomes a provider-scoped picker, defaulted to anthropic's
    // first catalogue model.
    const modelTrigger = screen.getByLabelText("Model");
    expect(modelTrigger.getAttribute("role")).toBe("combobox");
    expect(modelTrigger.textContent).toContain("claude-sonnet-4-6");
    // The credential name follows the detected provider until edited.
    expect((screen.getByLabelText(/credential name/i) as HTMLInputElement).value).toBe("anthropic");
  });

  it("lists only the selected provider's models in the scoped picker", () => {
    const catalogue: ModelCap[] = [
      ...MODELS,
      { id: "claude-opus-4-8", provider: "anthropic", context_cap_tokens: 256_000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    renderForm({ catalogue });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    const modelTrigger = screen.getByLabelText("Model");
    fireEvent.pointerDown(modelTrigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(modelTrigger);
    fireEvent.keyDown(modelTrigger, { key: "Enter" });
    // Anthropic's models are listed; the moonshot model is not.
    expect(screen.getByText("claude-opus-4-8")).toBeTruthy();
    expect(screen.queryByText("kimi-k2.6")).toBeNull();
  });

  it("keeps the current model when re-applying a provider that still offers it", () => {
    const catalogue: ModelCap[] = [
      ...MODELS,
      { id: "claude-opus-4-8", provider: "anthropic", context_cap_tokens: 256_000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    renderForm({ catalogue });
    // First apply sets the model to anthropic's first catalogue entry.
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    const trigger = screen.getByLabelText("Model");
    expect(trigger.textContent).toContain("claude-sonnet-4-6");
    // Re-applying the same provider (trailing space → distinct input event, same
    // trimmed provider) must KEEP the still-valid model, not reset it.
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic " } });
    expect(screen.getByLabelText("Model").textContent).toContain("claude-sonnet-4-6");
  });

  it("does not overwrite a provider the user typed when a key is pasted", () => {
    renderForm();
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "my-proxy" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-ant-secret123" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("my-proxy");
  });

  it("auto-names the credential after the provider until the name is edited", () => {
    renderForm();
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    expect((screen.getByLabelText(/credential name/i) as HTMLInputElement).value).toBe("anthropic");

    fireEvent.change(screen.getByLabelText(/credential name/i), { target: { value: "anthropic-prod" } });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic-eu" } });
    expect((screen.getByLabelText(/credential name/i) as HTMLInputElement).value).toBe("anthropic-prod");
  });

  it("submits {provider, api_key, model} under the auto-name and selects the new credential", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    const onCreated = vi.fn();
    renderForm({ catalogue: [], onCreated });

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
    renderForm({ catalogue: [], onCreated });

    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("submits on Enter in a field and ignores other keys", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    const onCreated = vi.fn();
    renderForm({ catalogue: [], onCreated });

    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    // A non-Enter key does nothing.
    fireEvent.keyDown(screen.getByLabelText("Model"), { key: "a" });
    expect(createCredentialActionMock).not.toHaveBeenCalled();
    // Enter stores the key.
    fireEvent.keyDown(screen.getByLabelText("Model"), { key: "Enter" });
    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith("anthropic"));
  });

  it("resets the model to the new provider's default when a pasted key changes the provider", () => {
    renderForm();
    // Before a provider is known the model is a free-text field.
    const modelInput = screen.getByLabelText("Model") as HTMLInputElement;
    expect(modelInput.tagName).toBe("INPUT");
    fireEvent.change(modelInput, { target: { value: "custom-model" } });
    // Pasting an anthropic key applies the provider and re-defaults the model to
    // anthropic's catalogue — the prior free-typed model belonged to no provider.
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-ant-xyz" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("anthropic");
    const modelTrigger = screen.getByLabelText("Model");
    expect(modelTrigger.getAttribute("role")).toBe("combobox");
    expect(modelTrigger.textContent).toContain("claude-sonnet-4-6");
  });

  it("leaves the provider blank when the key prefix is unrecognized", () => {
    renderForm();
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "unknown-key-format" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("");
  });

  it("does not submit on Enter when required fields are missing", () => {
    renderForm({ catalogue: [] });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    fireEvent.keyDown(screen.getByLabelText("Provider"), { key: "Enter" });
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("recovers if the create action throws — re-enables the button and surfaces the error", async () => {
    createCredentialActionMock.mockRejectedValue(new Error("network partition"));
    const onCreated = vi.fn();
    renderForm({ catalogue: [], onCreated });
    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    const button = screen.getByRole("button", { name: /save key/i });
    fireEvent.click(button);
    // try/catch/finally must clear pending (button not stuck) and show the error.
    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
    expect((button as HTMLButtonElement).disabled).toBe(false);
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("surfaces a generic message when a non-Error value is thrown", async () => {
    createCredentialActionMock.mockRejectedValue("opaque failure");
    renderForm({ catalogue: [], onCreated: vi.fn() });
    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
  });

  it("ignores a second Enter while a submit is already in flight (pending guard)", async () => {
    // Hold the action in-flight so `pending` stays true between the two Enters.
    let resolveCreate: (value: { ok: true; data: { name: string } }) => void = () => {};
    const inFlight = new Promise<{ ok: true; data: { name: string } }>((res) => {
      resolveCreate = res;
    });
    createCredentialActionMock.mockReturnValue(inFlight);
    const onCreated = vi.fn();
    renderForm({ catalogue: [], onCreated });
    fillKeyFields("anthropic", "sk-ant-secret", "claude-sonnet-4-6");

    const modelField = screen.getByLabelText("Model");
    fireEvent.keyDown(modelField, { key: "Enter" });
    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    // A second Enter before the first resolves must be swallowed by the guard.
    fireEvent.keyDown(modelField, { key: "Enter" });
    expect(createCredentialActionMock).toHaveBeenCalledTimes(1);

    resolveCreate({ ok: true, data: { name: "anthropic" } });
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  });
});
