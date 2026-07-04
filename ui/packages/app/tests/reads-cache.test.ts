import { describe, expect, it, vi } from "vitest";

// Both the Models page and the Secrets page read the tenant provider + secret
// list through their own React `cache()` wrappers (collapsing repeat reads
// within one server render) — two separate modules with identical shape, not
// one shared file. Each page's own test mocks its wrappers, so neither proves
// the wrapper itself delegates to the underlying API function with the
// arguments forwarded — that's what this file is for, for both modules.

const { getTenantProvider, listSecrets } = vi.hoisted(() => ({
  getTenantProvider: vi.fn(),
  listSecrets: vi.fn(),
}));

vi.mock("@/lib/api/tenant_provider", () => ({ getTenantProvider }));
vi.mock("@/lib/api/secrets", () => ({ listSecrets }));

import {
  getTenantProviderCached as getTenantProviderCachedModels,
  listSecretsCached as listSecretsCachedModels,
} from "@/app/(dashboard)/settings/models/lib/reads";
import {
  getTenantProviderCached as getTenantProviderCachedSecrets,
  listSecretsCached as listSecretsCachedSecrets,
} from "@/app/(dashboard)/secrets/lib/reads";

describe("Models cached reads", () => {
  it("getTenantProviderCached forwards the token to getTenantProvider", async () => {
    const provider = { mode: "platform" };
    getTenantProvider.mockResolvedValue(provider);
    await expect(getTenantProviderCachedModels("tok")).resolves.toBe(provider);
    expect(getTenantProvider).toHaveBeenCalledWith("tok");
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
