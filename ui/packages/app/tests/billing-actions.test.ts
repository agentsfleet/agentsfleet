import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// listTenantBillingChargesAction is a thin forwarder: it wraps
// withToken((t) => apiListTenantBillingCharges(t, opts)). We mock the token
// wrapper and the api client so the only thing under test is that the token is
// threaded into the leading position and opts are forwarded verbatim.

// vi.mock is hoisted above the static action import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run.
const { withTokenMock, listTenantBillingChargesMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  listTenantBillingChargesMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/tenant_billing", () => ({ listTenantBillingCharges: listTenantBillingChargesMock }));

import { listTenantBillingChargesAction } from "@/app/(dashboard)/settings/billing/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("listTenantBillingChargesAction — thin withToken forwarder", () => {
  it("threads the token and forwards explicit opts to the client, returning the wrapped result", async () => {
    const charges = { items: [{ id: "ch_1" }], next_cursor: null };
    listTenantBillingChargesMock.mockResolvedValueOnce(charges);
    const opts = { limit: 50, cursor: "cur_abc" };
    const r = await listTenantBillingChargesAction(opts);
    expect(r).toEqual({ ok: true, data: charges });
    expect(listTenantBillingChargesMock).toHaveBeenCalledWith("tok", opts);
  });

  it("defaults opts to an empty object when called with no argument", async () => {
    const charges = { items: [], next_cursor: null };
    listTenantBillingChargesMock.mockResolvedValueOnce(charges);
    const r = await listTenantBillingChargesAction();
    expect(r).toEqual({ ok: true, data: charges });
    expect(listTenantBillingChargesMock).toHaveBeenCalledWith("tok", {});
  });
});
