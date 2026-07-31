import { describe, expect, it } from "vitest";
import {
  POLICY_FORM_DEFAULTS,
  formFromPolicy,
  policyFormSchema,
  policyFromForm,
} from "./PolicyFields";
import type { AssignedPolicy } from "@/lib/api/runners";

// Pure-function coverage for the shared assignment form: the value mapping and
// the schema bounds are what both dialogs stand on, so their failure paths get
// direct tests here (no rendering needed).

const STORED: AssignedPolicy = {
  sandbox_tier: "container_nested",
  network_policy: "deny_all_egress",
  registry_allowlist: ["pypi.org", "registry.npmjs.org"],
  worker_count: 4,
};

describe("policyFromForm / formFromPolicy", () => {
  it("round-trips a stored assignment through the form shape unchanged", () => {
    const out = policyFromForm(formFromPolicy(STORED));
    expect(out.error).toBeNull();
    expect(out.policy).toEqual(STORED);
  });

  it("should surface the registry parse error and produce no policy on a bad entry", () => {
    const values = { ...POLICY_FORM_DEFAULTS, registry_allowlist: "pypi.org, not a host!" };
    const out = policyFromForm(values);
    expect(out.policy).toBeNull();
    expect(out.error).toContain("not a host!");
  });

  it("an empty registry field maps to an empty allowlist (runner defaults apply)", () => {
    const out = policyFromForm({ ...POLICY_FORM_DEFAULTS, registry_allowlist: "  " });
    expect(out.error).toBeNull();
    expect(out.policy?.registry_allowlist).toEqual([]);
  });
});

describe("policyFormSchema worker bounds", () => {
  it.each(["0", "65", "abc", "1.5", "-3", ""])(
    "should reject worker_count %j outside the shared [1, 64] bounds",
    (bad) => {
      const r = policyFormSchema.safeParse({ ...POLICY_FORM_DEFAULTS, worker_count: bad });
      expect(r.success).toBe(false);
    },
  );

  it.each(["1", "64", "8"])("should accept worker_count %j inside the bounds", (good) => {
    const r = policyFormSchema.safeParse({ ...POLICY_FORM_DEFAULTS, worker_count: good });
    expect(r.success).toBe(true);
  });
});
