import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SCOPE } from "@/lib/auth/scopes";

// ── Shared mocks ───────────────────────────────────────────────────────────
// The actions module is the dashboard's defence-in-depth gate: it must fail
// closed on the specific operator scope BEFORE any token round-trip (reads on
// model:read, mutations on model:admin — mirrors the runners admin actions). We
// mock the scope check, the token wrapper, the workspace resolver, and the API
// clients so only the action's own branches are under test — the real security
// boundary is the backend requireScope, proven by its integration suite.
// `@/lib/errors` stays real so INSUFFICIENT_SCOPE → UZ-AUTH-022 is the actual
// constant, not a fixture.

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run.
const {
  hasScopeMock,
  withTokenMock,
  withWorkspaceScopeMock,
  createCredentialMock,
  listAdminModelsMock,
  createAdminModelMock,
  updateAdminModelMock,
  deleteAdminModelMock,
  setPlatformDefaultMock,
} = vi.hoisted(() => ({
  hasScopeMock: vi.fn(),
  withTokenMock: vi.fn(),
  withWorkspaceScopeMock: vi.fn(),
  createCredentialMock: vi.fn(),
  listAdminModelsMock: vi.fn(),
  createAdminModelMock: vi.fn(),
  updateAdminModelMock: vi.fn(),
  deleteAdminModelMock: vi.fn(),
  setPlatformDefaultMock: vi.fn(),
}));

vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));
vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/workspace", () => ({ withWorkspaceScope: withWorkspaceScopeMock }));
vi.mock("@/lib/api/credentials", () => ({ createCredential: createCredentialMock }));
vi.mock("@/lib/api/admin_models", () => ({
  listAdminModels: listAdminModelsMock,
  createAdminModel: createAdminModelMock,
  updateAdminModel: updateAdminModelMock,
  deleteAdminModel: deleteAdminModelMock,
  setPlatformDefault: setPlatformDefaultMock,
}));

import {
  listAdminModelsAction,
  createAdminModelAction,
  updateAdminModelAction,
  deleteAdminModelAction,
  setPlatformDefaultAction,
} from "@/app/(dashboard)/admin/models/actions";

const MODEL = {
  uid: "u1",
  provider: "fireworks",
  model_id: "glm-5.2",
  context_cap_tokens: 128000,
  input_nanos_per_mtok: 550_000_000,
  cached_input_nanos_per_mtok: 140_000_000,
  output_nanos_per_mtok: 2_190_000_000,
};

beforeEach(() => {
  vi.clearAllMocks();
  // Faithful to the real withToken: forward a resolved token, normalise a
  // thrown error into { ok: false } rather than letting it escape the action.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => {
    try {
      return { ok: true, data: await fn("tok") };
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
  });
});
afterEach(() => vi.resetAllMocks());

function notScoped(scope: string) {
  return { ok: false, error: `Operator scope required: ${scope}`, status: 403, errorCode: "UZ-AUTH-022" };
}

// test_admin_models_ui_gates_on_model_scope_no_boolean_left (Dimension 4.2)
describe("admin/models server actions — per-scope gate (defence-in-depth)", () => {
  it("listAdminModelsAction gates on model:read and fails closed 403 UZ-AUTH-022 without it", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    expect(await listAdminModelsAction()).toEqual(notScoped(SCOPE.MODEL_READ));
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.MODEL_READ);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(listAdminModelsMock).not.toHaveBeenCalled();
  });

  it("createAdminModelAction gates on model:admin and fails closed without it, before any round-trip", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    expect(await createAdminModelAction({ ...MODEL })).toEqual(notScoped(SCOPE.MODEL_ADMIN));
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.MODEL_ADMIN);
    expect(createAdminModelMock).not.toHaveBeenCalled();
  });

  it("setPlatformDefaultAction gates on model:admin and fails closed before storing any key", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const r = await setPlatformDefaultAction({ provider: "fireworks", model: "glm-5.2", api_key: "sk-x" });
    expect(r).toEqual(notScoped(SCOPE.MODEL_ADMIN));
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.MODEL_ADMIN);
    // The api_key never reaches the vault when the gate fails closed.
    expect(createCredentialMock).not.toHaveBeenCalled();
    expect(setPlatformDefaultMock).not.toHaveBeenCalled();
  });
});

describe("admin/models server actions — scoped happy paths forward through withToken", () => {
  beforeEach(() => hasScopeMock.mockResolvedValue(true));

  it("listAdminModelsAction forwards the token to the client", async () => {
    listAdminModelsMock.mockResolvedValueOnce({ models: [MODEL] });
    expect(await listAdminModelsAction()).toEqual({ ok: true, data: { models: [MODEL] } });
    expect(listAdminModelsMock).toHaveBeenCalledWith("tok");
  });

  it("createAdminModelAction forwards the cap body through withToken", async () => {
    createAdminModelMock.mockResolvedValueOnce(MODEL);
    const body = {
      provider: "fireworks",
      model_id: "glm-5.2",
      context_cap_tokens: 128000,
      input_nanos_per_mtok: 550_000_000,
      cached_input_nanos_per_mtok: 140_000_000,
      output_nanos_per_mtok: 2_190_000_000,
    };
    expect(await createAdminModelAction(body)).toEqual({ ok: true, data: MODEL });
    expect(createAdminModelMock).toHaveBeenCalledWith("tok", body);
  });

  it("updateAdminModelAction forwards the uid + rates body through withToken", async () => {
    updateAdminModelMock.mockResolvedValueOnce({ uid: "u1", updated: true });
    const rates = {
      context_cap_tokens: 256000,
      input_nanos_per_mtok: 600_000_000,
      cached_input_nanos_per_mtok: 150_000_000,
      output_nanos_per_mtok: 2_300_000_000,
    };
    expect(await updateAdminModelAction("u1", rates)).toEqual({ ok: true, data: { uid: "u1", updated: true } });
    expect(updateAdminModelMock).toHaveBeenCalledWith("tok", "u1", rates);
  });

  it("deleteAdminModelAction forwards the uid through withToken", async () => {
    deleteAdminModelMock.mockResolvedValueOnce(undefined);
    expect(await deleteAdminModelAction("u1")).toEqual({ ok: true, data: undefined });
    expect(deleteAdminModelMock).toHaveBeenCalledWith("tok", "u1");
  });
});

describe("setPlatformDefaultAction — two-step vault write + activation", () => {
  beforeEach(() => hasScopeMock.mockResolvedValue(true));

  it("stores the key in the admin workspace vault then activates the catalogued default (no base_url)", async () => {
    withWorkspaceScopeMock.mockImplementationOnce(
      async (_t: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws-1"),
    );
    setPlatformDefaultMock.mockResolvedValueOnce({ provider: "fireworks", model: "glm-5.2", active: true });

    const r = await setPlatformDefaultAction({ provider: "fireworks", model: "glm-5.2", api_key: "sk-secret" });

    expect(r).toEqual({ ok: true, data: { provider: "fireworks", model: "glm-5.2", active: true } });
    // The key is written to the acting admin's workspace under the provider name;
    // base_url is omitted from the vault payload when not provided.
    expect(createCredentialMock).toHaveBeenCalledWith(
      "ws-1",
      { name: "fireworks", data: { provider: "fireworks", api_key: "sk-secret", model: "glm-5.2" } },
      "tok",
    );
    expect(setPlatformDefaultMock).toHaveBeenCalledWith("tok", {
      provider: "fireworks",
      source_workspace_id: "ws-1",
      model: "glm-5.2",
      base_url: undefined,
    });
  });

  it("threads base_url into both the vault payload and the activation for an openai-compatible default", async () => {
    withWorkspaceScopeMock.mockImplementationOnce(
      async (_t: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws-9"),
    );
    setPlatformDefaultMock.mockResolvedValueOnce({ provider: "openai-compatible", model: "glm-5.2", active: true });

    const r = await setPlatformDefaultAction({
      provider: "openai-compatible",
      model: "glm-5.2",
      api_key: "sk-secret",
      base_url: "https://endpoint.example/v1",
    });

    expect(r.ok).toBe(true);
    expect(createCredentialMock).toHaveBeenCalledWith(
      "ws-9",
      {
        name: "openai-compatible",
        data: {
          provider: "openai-compatible",
          api_key: "sk-secret",
          model: "glm-5.2",
          base_url: "https://endpoint.example/v1",
        },
      },
      "tok",
    );
    expect(setPlatformDefaultMock).toHaveBeenCalledWith("tok", {
      provider: "openai-compatible",
      source_workspace_id: "ws-9",
      model: "glm-5.2",
      base_url: "https://endpoint.example/v1",
    });
  });

  it("fails with a clear error when there is no active workspace to store the key in", async () => {
    withWorkspaceScopeMock.mockResolvedValueOnce(null);

    const r = await setPlatformDefaultAction({ provider: "fireworks", model: "glm-5.2", api_key: "sk-secret" });

    expect(r).toEqual({ ok: false, error: "No active workspace to store the platform key in" });
    // No vault write, no activation when the workspace can't be resolved.
    expect(createCredentialMock).not.toHaveBeenCalled();
    expect(setPlatformDefaultMock).not.toHaveBeenCalled();
  });
});
