import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EVENTS } from "@/lib/analytics/events";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";

// The consolidated "add an OpenAI-compatible endpoint" form (settings/models —
// distinct from the deleted credentials one). Stores
// `{provider: "openai-compatible", base_url, model, api_key?}` with the api key
// optional, gates a non-https URL inline before any request, and on `activate`
// points the tenant provider at the new endpoint.

const routerRefresh = vi.fn();
const createSecretAction = vi.hoisted(() => vi.fn());
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const captureModelActivated = vi.hoisted(() => vi.fn());
const captureProductEvent = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/secrets/actions", () => ({ createSecretAction }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ setProviderSelfManagedAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureModelActivated }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent }));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderModelSelect", async () => (await import("./helpers/models-component-mocks")).providerModelSelectStub());

import CustomEndpointForm from "@/app/(dashboard)/settings/models/components/CustomEndpointForm";

beforeEach(() => {
  vi.clearAllMocks();
  createSecretAction.mockResolvedValue({ ok: true, data: { name: "vllm" } });
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: OPENAI_COMPATIBLE_PROVIDER, mode: "self_managed", model: "m1" },
  });
});
afterEach(() => cleanup());

function fill(name: string, baseUrl: string, model: string, key?: string) {
  fireEvent.change(screen.getByLabelText("Name"), { target: { value: name } });
  fireEvent.change(screen.getByLabelText("Base URL"), { target: { value: baseUrl } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: model } });
  if (key !== undefined) fireEvent.change(screen.getByLabelText("API key (optional)"), { target: { value: key } });
}

describe("CustomEndpointForm", () => {
  it("flags a non-https base URL inline and makes no request", async () => {
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", onDone: vi.fn() }));
    fill("vllm", "http://vllm.corp/v1", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Add custom endpoint" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/https/i));
    expect(createSecretAction).not.toHaveBeenCalled();
  });

  it("stores the endpoint without an api key and activates it", async () => {
    const onDone = vi.fn();
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", activate: true, onDone }));
    fill("vllm", "https://vllm.corp/v1", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save & make active" }));
    await waitFor(() =>
      expect(createSecretAction).toHaveBeenCalledWith("ws_1", {
        name: "vllm",
        data: { provider: OPENAI_COMPATIBLE_PROVIDER, base_url: "https://vllm.corp/v1", model: "m1" },
      }),
    );
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.secret_added, { secret_name: "vllm" });
    expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "vllm", model: "m1" });
    expect(captureModelActivated).toHaveBeenCalled();
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("includes the api key when supplied; no activation when `activate` is unset", async () => {
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", onDone: vi.fn() }));
    fill("vllm", "https://vllm.corp/v1", "m1", "sk-secret");
    fireEvent.click(screen.getByRole("button", { name: "Add custom endpoint" }));
    await waitFor(() =>
      expect(createSecretAction).toHaveBeenCalledWith("ws_1", {
        name: "vllm",
        data: { provider: OPENAI_COMPATIBLE_PROVIDER, base_url: "https://vllm.corp/v1", model: "m1", api_key: "sk-secret" },
      }),
    );
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
  });

  it("surfaces a store error", async () => {
    createSecretAction.mockResolvedValue({ ok: false, error: "too big" });
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", onDone: vi.fn() }));
    fill("vllm", "https://vllm.corp/v1", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Add custom endpoint" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/Couldn't store the custom endpoint/i));
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
  });

  it("surfaces an activation error after a successful store", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "activation rejected" });
    const onDone = vi.fn();
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", activate: true, onDone }));
    fill("vllm", "https://vllm.corp/v1", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save & make active" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/activation rejected/));
    expect(onDone).not.toHaveBeenCalled();
  });

  it("does nothing when incomplete (Enter on an empty field) and exposes Cancel", () => {
    const onCancel = vi.fn();
    render(React.createElement(CustomEndpointForm, { workspaceId: "ws_1", onDone: vi.fn(), onCancel }));
    // A non-Enter key is ignored by the field handler.
    fireEvent.keyDown(screen.getByLabelText("Name"), { key: "a" });
    fireEvent.keyDown(screen.getByLabelText("Name"), { key: "Enter" });
    expect(createSecretAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
