import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { EVENTS } from "../lib/analytics/events";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  CREDENTIAL_FIELD,
} from "../lib/types";

// ── mocks (hoisted: vi.mock factories run before imports) ──────────────────
const { createCredentialActionMock, routerRefresh, captureProductEventMock } = vi.hoisted(() => ({
  createCredentialActionMock: vi.fn(),
  routerRefresh: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }),
}));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: createCredentialActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));
vi.mock("lucide-react", () => ({
  Loader2Icon: (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": "Loader2Icon" }),
}));

import CustomEndpointForm, {
  isHttpsUrl,
  BASE_URL_NOT_HTTPS,
} from "@/app/(dashboard)/credentials/components/CustomEndpointForm";

const WORKSPACE_ID = "ws_custom_endpoint";
const HTTPS_BASE_URL = "https://vllm.corp/v1";
const HTTP_BASE_URL = "http://vllm.corp/v1";
const API_KEY = "sk-custom-key";
const CRED_NAME = "vllm-gateway";
const MODEL = "claude-opus-4-8";

beforeEach(() => {
  createCredentialActionMock.mockReset();
  routerRefresh.mockReset();
  captureProductEventMock.mockReset();
});
afterEach(() => cleanup());

function fill(name: string, baseUrl: string, apiKey?: string, model: string = MODEL) {
  fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: name } });
  fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: baseUrl } });
  fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: model } });
  if (apiKey !== undefined) {
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: apiKey } });
  }
}

describe("isHttpsUrl", () => {
  it("accepts an https URL", () => {
    expect(isHttpsUrl("https://vllm.corp/v1")).toBe(true);
  });
  it("rejects an http URL", () => {
    expect(isHttpsUrl("http://vllm.corp/v1")).toBe(false);
  });
  it("rejects a malformed value (no https prefix)", () => {
    expect(isHttpsUrl("not a url")).toBe(false);
  });
  it("rejects an https-prefixed but unparseable value (hits the URL-throw catch)", () => {
    // Starts with the prefix, so it reaches `new URL`, which throws → false.
    expect(isHttpsUrl("https://")).toBe(false);
  });
  it("rejects a non-http scheme that lacks the https prefix", () => {
    expect(isHttpsUrl("ftp://vllm.corp/v1")).toBe(false);
  });
});

describe("CustomEndpointForm", () => {
  it("test_custom_credential_form_payload: submit → createCredential body has provider + base_url + model", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: CRED_NAME } });
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL, API_KEY);
    fireEvent.click(screen.getByRole("button", { name: /add custom endpoint/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, {
      name: CRED_NAME,
      data: {
        [CREDENTIAL_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [CREDENTIAL_FIELD.baseUrl]: HTTPS_BASE_URL,
        [CREDENTIAL_FIELD.model]: MODEL,
        [CREDENTIAL_FIELD.apiKey]: API_KEY,
      },
    });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.credential_added, {
      credential_name: CRED_NAME,
    });
    // The secret key must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain(API_KEY);
  });

  it("omits api_key from the payload when the key field is left blank (model still required)", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: CRED_NAME } });
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL);
    fireEvent.click(screen.getByRole("button", { name: /add custom endpoint/i }));

    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledWith(WORKSPACE_ID, {
      name: CRED_NAME,
      data: {
        [CREDENTIAL_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [CREDENTIAL_FIELD.baseUrl]: HTTPS_BASE_URL,
        [CREDENTIAL_FIELD.model]: MODEL,
      },
    });
  });

  it("flags a non-https base URL inline and never calls createCredential", async () => {
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTP_BASE_URL, API_KEY);
    fireEvent.click(screen.getByRole("button", { name: /add custom endpoint/i }));

    await waitFor(() => expect(screen.getByRole("alert").textContent).toBe(BASE_URL_NOT_HTTPS));
    expect(createCredentialActionMock).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("surfaces a server error and does not refresh", async () => {
    createCredentialActionMock.mockResolvedValue({
      ok: false,
      error: "blocked_host",
      errorCode: "UZ-PROVIDER-005",
      status: 400,
    });
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL, API_KEY);
    fireEvent.click(screen.getByRole("button", { name: /add custom endpoint/i }));

    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("blocked_host"));
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("disables the submit button until name + base URL + model are filled", () => {
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    const button = screen.getByRole("button", { name: /add custom endpoint/i }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    // Name + base URL alone are not enough — the resolver requires a model, so
    // the submit stays disabled until the model field is filled too.
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: CRED_NAME } });
    fireEvent.change(screen.getByLabelText(/base url/i), { target: { value: HTTPS_BASE_URL } });
    expect(button.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: MODEL } });
    expect(button.disabled).toBe(false);
  });

  it("invokes onCreated with the stored name after a successful submit", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: CRED_NAME } });
    const onCreated = vi.fn();
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID, onCreated }));
    fill(CRED_NAME, HTTPS_BASE_URL);
    fireEvent.click(screen.getByRole("button", { name: /add custom endpoint/i }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(CRED_NAME));
  });

  it("Enter on a field submits the endpoint", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: CRED_NAME } });
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL);
    fireEvent.keyDown(screen.getByLabelText(/base url/i), { key: "Enter" });
    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
  });

  it("ignores other keys on a field (only Enter submits)", () => {
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL);
    fireEvent.keyDown(screen.getByLabelText(/base url/i), { key: "a" });
    expect(createCredentialActionMock).not.toHaveBeenCalled();
  });

  it("ignores a second Enter while a submit is in flight (pending guard)", async () => {
    let resolveSave!: (v: { ok: true; data: { name: string } }) => void;
    createCredentialActionMock.mockReturnValue(
      new Promise<{ ok: true; data: { name: string } }>((r) => { resolveSave = r; }),
    );
    render(React.createElement(CustomEndpointForm, { workspaceId: WORKSPACE_ID }));
    fill(CRED_NAME, HTTPS_BASE_URL);
    const field = screen.getByLabelText(/base url/i);
    fireEvent.keyDown(field, { key: "Enter" }); // enters pending
    fireEvent.keyDown(field, { key: "Enter" }); // guarded — no second call
    await waitFor(() => expect(createCredentialActionMock).toHaveBeenCalledTimes(1));
    expect(createCredentialActionMock).toHaveBeenCalledTimes(1);
    await act(async () => { resolveSave({ ok: true, data: { name: CRED_NAME } }); });
  });
});
