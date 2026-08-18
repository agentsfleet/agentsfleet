import { describe, expect, it } from "vitest";
import {
  POLICY_FORM_DEFAULTS,
  formFromPolicy,
  policyFormSchema,
  policyFromForm,
} from "./PolicyFields";
import { MAX_BIND_NOTE_LEN, MAX_EXTRA_BINDS } from "./policy-binds";
import type { AssignedPolicy } from "@/lib/api/runners";

// Pure-function coverage for the shared assignment form: the value mapping and
// the schema bounds are what both dialogs stand on, so their failure paths get
// direct tests here (no rendering needed).

const STORED: AssignedPolicy = {
  sandbox_tier: "container_nested",
  network_policy: "deny_all_egress",
  registry_allowlist: ["pypi.org", "registry.npmjs.org"],
  worker_count: 4,
  extra_binds: [{ path: "/srv/models", mode: "read_only", note: "gpu weights" }],
};

describe("policyFromForm / formFromPolicy", () => {
  it("round-trips a stored assignment through the form shape unchanged", () => {
    const out = policyFromForm(formFromPolicy(STORED));
    expect(out.error).toBeNull();
    expect(out.policy).toEqual(STORED);
  });

  it("carries existing binds through an edit that never touches them", () => {
    // PATCH replaces the WHOLE assignment, so a form that drops extra_binds
    // sends [] and wipes the operator's mounts. The round-trip is the guard.
    const values = { ...formFromPolicy(STORED), worker_count: "8" };
    expect(policyFromForm(values).policy?.extra_binds).toEqual(STORED.extra_binds);
  });

  it("maps a runner with no binds to an empty list, never to a missing key", () => {
    const bare: AssignedPolicy = { ...STORED, extra_binds: undefined };
    expect(policyFromForm(formFromPolicy(bare)).policy?.extra_binds).toEqual([]);
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

describe("policyFormSchema bind rows", () => {
  function withBinds(rows: { path: string; mode: "read_only" | "read_write"; note: string }[]) {
    return policyFormSchema.safeParse({ ...POLICY_FORM_DEFAULTS, extra_binds: rows });
  }

  it("accepts a well-formed additive bind", () => {
    expect(withBinds([{ path: "/srv/models", mode: "read_only", note: "gpu weights" }]).success).toBe(true);
  });

  it("accepts a blank row — it is dropped on save, not refused", () => {
    expect(withBinds([{ path: "  ", mode: "read_only", note: "" }]).success).toBe(true);
  });

  it("refuses a path the daemon already binds, and points the issue at that row", () => {
    const r = withBinds([
      { path: "/srv/models", mode: "read_only", note: "" },
      { path: "/etc/ssl", mode: "read_only", note: "" },
    ]);
    expect(r.success).toBe(false);
    expect(r.error?.issues[0]?.path).toEqual(["extra_binds", 1, "path"]);
  });

  it("refuses an over-long note against the row that carries it", () => {
    const r = withBinds([{ path: "/srv/models", mode: "read_only", note: "n".repeat(MAX_BIND_NOTE_LEN + 1) }]);
    expect(r.success).toBe(false);
    expect(r.error?.issues[0]?.path).toEqual(["extra_binds", 0, "note"]);
  });

  it("refuses more filled rows than the shared cap, naming the bound", () => {
    const rows = Array.from({ length: MAX_EXTRA_BINDS + 1 }, (_, i) => ({
      path: `/srv/models-${i}`,
      mode: "read_only" as const,
      note: "",
    }));
    const r = withBinds(rows);
    expect(r.success).toBe(false);
    expect(r.error?.issues.some((issue) => issue.message.includes(String(MAX_EXTRA_BINDS)))).toBe(true);
  });
});
