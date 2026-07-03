import { describe, expect, it, vi } from "vitest";

// The Models page reads the tenant provider + credential list through
// React `cache()` wrappers (collapsing repeat reads within one server render).
// The page test mocks these wrappers, so this proves the wrappers themselves
// delegate to the underlying API functions with the arguments forwarded.

const { getTenantProvider, listCredentials } = vi.hoisted(() => ({
  getTenantProvider: vi.fn(),
  listCredentials: vi.fn(),
}));

vi.mock("@/lib/api/tenant_provider", () => ({ getTenantProvider }));
vi.mock("@/lib/api/credentials", () => ({ listCredentials }));

import {
  getTenantProviderCached,
  listCredentialsCached,
} from "@/app/(dashboard)/settings/models/lib/reads";

describe("Models cached reads", () => {
  it("getTenantProviderCached forwards the token to getTenantProvider", async () => {
    const provider = { mode: "platform" };
    getTenantProvider.mockResolvedValue(provider);
    await expect(getTenantProviderCached("tok")).resolves.toBe(provider);
    expect(getTenantProvider).toHaveBeenCalledWith("tok");
  });

  it("listCredentialsCached forwards the workspace id + token to listCredentials", async () => {
    const resp = { credentials: [] };
    listCredentials.mockResolvedValue(resp);
    await expect(listCredentialsCached("ws_1", "tok")).resolves.toBe(resp);
    expect(listCredentials).toHaveBeenCalledWith("ws_1", "tok");
  });
});
