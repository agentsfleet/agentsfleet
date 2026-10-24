import type { TenantModelEntry } from "@/lib/types";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { PROVIDER_MODE } from "@/lib/types";
import { HTTP_METHOD } from "@/lib/api/retry";
import { SECRET_KIND } from "@/lib/api/secrets-types";
import { CATALOGUE_MODEL, VERIFIED_AT } from "./helpers/catalogue-fixtures";
import ModelsRegistryTable from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable";
import { ModelCatalogueProvider } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider";

vi.mock("@clerk/nextjs/server", () => ({
  auth: async () => ({ getToken: async () => "unit-test-bearer" }),
}));
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn() }) }));

const WORKSPACE_ID = "ws-created-model";
const SECRET_REF = "created-model-key";
const MODEL_ID = CATALOGUE_MODEL.id;
const PLATFORM_MODEL = "platform-default-model";
const POST = "POST";
const PATH = {
  catalogue: "/v1/models",
  registry: "/v1/tenants/me/models",
  provider: "/v1/tenants/me/provider",
  secrets: `/v1/workspaces/${WORKSPACE_ID}/secrets`,
} as const;
const COPY = {
  active: "Active", create: "Create model", saveActive: "Save & make active",
  activationRefused: "Activation refused", registrationRefused: "Registration refused",
} as const;
const SWITCH_LABEL = `Switch to ${MODEL_ID}`;
const STATUS_CONFLICT = 409;
const STATUS_UNPROCESSABLE = 422;

class ModelBoundary {
  readonly activation = Promise.withResolvers<Response>();
  readonly calls: string[] = [];
  readonly unexpected: string[] = [];
  saved = false;
  registered = false;
  active = false;
  refuseRegistration = false;

  entry(): TenantModelEntry {
    return {
      id: "created-model-entry", model_id: MODEL_ID, display_name: CATALOGUE_MODEL.display_name,
      provider: CATALOGUE_MODEL.provider, secret_ref: SECRET_REF, kind: SECRET_KIND.provider_key,
      has_key: true, active: this.active, created_at: VERIFIED_AT,
    };
  }

  page() {
    return {
      models: this.registered ? [this.entry()] : [], next_cursor: null, total: this.registered ? 1 : 0,
      platform_default_available: true,
      platform_default: { provider: CATALOGUE_MODEL.provider, model: PLATFORM_MODEL, context_cap_tokens: CATALOGUE_MODEL.context_cap_tokens },
    };
  }

  fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const source = input instanceof Request ? input.url : input;
    const path = new URL(source, "https://unit.invalid").pathname.replace(/^\/backend/, "");
    const method = init?.method ?? HTTP_METHOD.GET;
    this.calls.push(`${method} ${path}`);
    if (path === PATH.catalogue && method === HTTP_METHOD.GET) {
      return Response.json({ version: VERIFIED_AT, models: [CATALOGUE_MODEL], next_cursor: null, total: 1 });
    }
    if (path === PATH.secrets && method === HTTP_METHOD.GET) {
      return Response.json({ secrets: this.saved ? [{ name: SECRET_REF, kind: SECRET_KIND.provider_key, provider: CATALOGUE_MODEL.provider, created_at: VERIFIED_AT }] : [] });
    }
    if (path === PATH.secrets && method === POST) {
      this.saved = true;
      return Response.json({ name: SECRET_REF });
    }
    if (path === PATH.registry && method === POST) {
      expect(requestBody(init)).toEqual({ model_id: MODEL_ID, secret_ref: SECRET_REF });
      if (this.refuseRegistration) return Response.json({ error_code: "UZ-MODELS-003", detail: COPY.registrationRefused }, { status: STATUS_CONFLICT });
      this.registered = true;
      return Response.json(this.entry());
    }
    if (path === PATH.registry && method === HTTP_METHOD.GET) return Response.json(this.page());
    if (path === PATH.provider && method === HTTP_METHOD.PUT) {
      expect(requestBody(init)).toEqual({ mode: PROVIDER_MODE.self_managed, model: MODEL_ID, secret_ref: SECRET_REF });
      return this.activation.promise;
    }
    this.unexpected.push(`${method} ${path}`);
    throw new Error(`Unexpected request: ${method} ${path}`);
  };

  completeActivation(success: boolean) {
    this.active = success;
    this.activation.resolve(success
      ? Response.json({ mode: PROVIDER_MODE.self_managed, provider: CATALOGUE_MODEL.provider, model: MODEL_ID, secret_ref: SECRET_REF, context_cap_tokens: CATALOGUE_MODEL.context_cap_tokens, platform_default_available: true })
      : Response.json({ error_code: "UZ-PROVIDER-003", detail: COPY.activationRefused }, { status: STATUS_UNPROCESSABLE }));
  }
}

function requestBody(init?: RequestInit): unknown {
  if (typeof init?.body !== "string") throw new Error("Expected a JSON request body");
  return JSON.parse(init.body);
}

function rowFor(model: string): HTMLElement {
  const rows = within(screen.getByRole("table", { hidden: true })).getAllByRole("row", { hidden: true });
  const row = rows.find((candidate) => candidate.textContent?.includes(model) ||
    Array.from(candidate.querySelectorAll("[aria-label]")).some((control) => control.getAttribute("aria-label")?.includes(model)));
  if (!row) throw new Error(`Missing model row: ${model}`);
  return row;
}

async function openAndSubmit(boundary: ModelBoundary) {
  vi.stubGlobal("fetch", boundary.fetch);
  render(<TooltipProvider><ModelCatalogueProvider>
    <ModelsRegistryTable workspaceId={WORKSPACE_ID} initialPage={boundary.page()} initialError={null} />
  </ModelCatalogueProvider></TooltipProvider>);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: COPY.create }));
  const dialog = within(screen.getByRole("dialog"));
  await user.type(dialog.getByLabelText(/^name$/i), SECRET_REF);
  await user.click(dialog.getByLabelText(/^provider$/i));
  await user.click(await screen.findByRole("option", { name: /fireworks/i }));
  await user.click(dialog.getByLabelText(/^model$/i));
  await user.click(await screen.findByRole("option", { name: new RegExp(MODEL_ID, "i") }));
  await user.type(dialog.getByLabelText(/^api key$/i), crypto.randomUUID());
  await user.click(dialog.getByRole("button", { name: COPY.saveActive }));
  return user;
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("created model activation through the registry and HTTP boundary", () => {
  it("refreshes the visible active row after the registration read has already completed", async () => {
    const boundary = new ModelBoundary();
    await openAndSubmit(boundary);
    // Force the registration read to render before activation commits.
    await waitFor(() => expect(within(rowFor(MODEL_ID)).queryByText(COPY.active)).toBeNull());
    expect(within(rowFor(PLATFORM_MODEL)).getByText(COPY.active)).toBeTruthy();
    expect(within(rowFor(MODEL_ID)).getByLabelText(SWITCH_LABEL)).toBeTruthy();
    expect(boundary.calls).toContain(`${HTTP_METHOD.PUT} ${PATH.provider}`);

    await act(async () => boundary.completeActivation(true));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(within(rowFor(MODEL_ID)).getByText(COPY.active)).toBeTruthy());
    expect(within(rowFor(MODEL_ID)).queryByLabelText(SWITCH_LABEL)).toBeNull();
    expect(within(rowFor(PLATFORM_MODEL)).queryByText(COPY.active)).toBeNull();
    expect(boundary.registered).toBe(true);
    expect(boundary.active).toBe(true);
    expect(boundary.unexpected).toEqual([]);
    expect(boundary.calls.filter((call) => call === `${POST} ${PATH.registry}`)).toHaveLength(1);
  });

  it("retains the committed inactive entry and dialog error when activation is refused", async () => {
    const boundary = new ModelBoundary();
    await openAndSubmit(boundary);
    await waitFor(() => expect(within(rowFor(MODEL_ID)).getByLabelText(SWITCH_LABEL)).toBeTruthy());

    await act(async () => boundary.completeActivation(false));

    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(within(dialog).getByRole("alert").textContent).toBe(`Couldn't activate this model — ${COPY.activationRefused}.`));
    expect(within(rowFor(MODEL_ID)).queryByText(COPY.active)).toBeNull();
    expect(within(rowFor(MODEL_ID)).getByLabelText(SWITCH_LABEL)).toBeTruthy();
    expect(within(rowFor(PLATFORM_MODEL)).getByText(COPY.active)).toBeTruthy();
    expect(boundary.saved).toBe(true);
    expect(boundary.registered).toBe(true);
    expect(boundary.active).toBe(false);
    expect(boundary.unexpected).toEqual([]);
    expect(boundary.calls.filter((call) => call === `${POST} ${PATH.registry}`)).toHaveLength(1);
  });

  it("creates no registry row or activation after registration is refused", async () => {
    const boundary = new ModelBoundary();
    boundary.refuseRegistration = true;
    await openAndSubmit(boundary);

    const dialog = screen.getByRole("dialog");
    await waitFor(() => expect(within(dialog).getByRole("alert").textContent).toBe(`Couldn't register the model entry — ${COPY.registrationRefused}.`));
    expect(within(screen.getByRole("table", { hidden: true })).queryByText(CATALOGUE_MODEL.display_name)).toBeNull();
    expect(within(rowFor(PLATFORM_MODEL)).getByText(COPY.active)).toBeTruthy();
    // Credential storage precedes registration and survives its refusal.
    expect(boundary.saved).toBe(true);
    expect(boundary.registered).toBe(false);
    expect(boundary.active).toBe(false);
    expect(boundary.calls).not.toContain(`${HTTP_METHOD.PUT} ${PATH.provider}`);
    expect(boundary.unexpected).toEqual([]);
  });
});
