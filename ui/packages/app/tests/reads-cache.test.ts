import { describe, expect, it, vi } from "vitest";

// Both the Models page and the Secrets page read their own secret list (and,
// for Secrets, the tenant provider) through their own React `cache()`
// wrappers (collapsing repeat reads within one server render) — two separate
// modules with identical shape, not one shared file. Each page's own test
// mocks its wrappers, so neither proves the wrapper itself delegates to the
// underlying API function with the arguments forwarded — that's what this
// file is for, for both modules. The Models page (M121) reads the tenant
// model registry instead of the tenant provider — the registry list already
// carries `active` per entry + `platform_default_available`.

const { getTenantProvider, listSecrets, listTenantModelEntries } = vi.hoisted(() => ({
  getTenantProvider: vi.fn(),
  listSecrets: vi.fn(),
  listTenantModelEntries: vi.fn(),
}));

vi.mock("@/lib/api/tenant_provider", () => ({ getTenantProvider }));
vi.mock("@/lib/api/secrets", () => ({ listSecrets }));
vi.mock("@/lib/api/tenant_model_entries", () => ({ listTenantModelEntries }));

import {
  listSecretsCached as listSecretsCachedModels,
  listTenantModelEntriesCached,
} from "@/app/(dashboard)/w/[workspaceId]/settings/models/lib/reads";
import {
  getTenantProviderCached as getTenantProviderCachedSecrets,
  listSecretsCached as listSecretsCachedSecrets,
} from "@/app/(dashboard)/w/[workspaceId]/secrets/lib/reads";

describe("Models cached reads", () => {
  it("listTenantModelEntriesCached forwards the token to listTenantModelEntries", async () => {
    const registry = { models: [], platform_default_available: true };
    listTenantModelEntries.mockResolvedValue(registry);
    await expect(listTenantModelEntriesCached("tok")).resolves.toBe(registry);
    expect(listTenantModelEntries).toHaveBeenCalledWith("tok");
  });

  it("listSecretsCached forwards the workspace id + token to listSecrets", async () => {
    const resp = { secrets: [] };
    listSecrets.mockResolvedValue(resp);
    await expect(listSecretsCachedModels("ws_1", "tok")).resolves.toBe(resp);
    expect(listSecrets).toHaveBeenCalledWith("ws_1", "tok");
  });
});

describe("Secrets cached reads", () => {
  it("getTenantProviderCached forwards the token to getTenantProvider", async () => {
    const provider = { mode: "self_managed" };
    getTenantProvider.mockResolvedValue(provider);
    await expect(getTenantProviderCachedSecrets("tok2")).resolves.toBe(provider);
    expect(getTenantProvider).toHaveBeenCalledWith("tok2");
  });

  it("listSecretsCached forwards the workspace id + token to listSecrets", async () => {
    const resp = { secrets: [{ name: "fly" }] };
    listSecrets.mockResolvedValue(resp);
    await expect(listSecretsCachedSecrets("ws_2", "tok2")).resolves.toBe(resp);
    expect(listSecrets).toHaveBeenCalledWith("ws_2", "tok2");
  });
});
