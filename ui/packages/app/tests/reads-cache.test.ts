import { describe, expect, it, vi } from "vitest";

// The Models page reads the tenant provider + secret list through
// React `cache()` wrappers (collapsing repeat reads within one server render).
// The page test mocks these wrappers, so this proves the wrappers themselves
// delegate to the underlying API functions with the arguments forwarded.

const { getTenantProvider, listSecrets } = vi.hoisted(() => ({
  getTenantProvider: vi.fn(),
  listSecrets: vi.fn(),
}));

vi.mock("@/lib/api/tenant_provider", () => ({ getTenantProvider }));
vi.mock("@/lib/api/secrets", () => ({ listSecrets }));

import {
  getTenantProviderCached,
  listSecretsCached,
} from "@/app/(dashboard)/settings/models/lib/reads";

describe("Models cached reads", () => {
  it("getTenantProviderCached forwards the token to getTenantProvider", async () => {
    const provider = { mode: "platform" };
    getTenantProvider.mockResolvedValue(provider);
    await expect(getTenantProviderCached("tok")).resolves.toBe(provider);
    expect(getTenantProvider).toHaveBeenCalledWith("tok");
  });

  it("listSecretsCached forwards the workspace id + token to listSecrets", async () => {
    const resp = { secrets: [] };
    listSecrets.mockResolvedValue(resp);
    await expect(listSecretsCached("ws_1", "tok")).resolves.toBe(resp);
    expect(listSecrets).toHaveBeenCalledWith("ws_1", "tok");
  });
});
