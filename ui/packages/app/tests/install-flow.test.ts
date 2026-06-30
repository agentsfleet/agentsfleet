import { describe, expect, it } from "vitest";
import {
  flowError,
  readyToCreate,
  requirementsOf,
  STATE_GLYPH,
  stepLine,
  unmetCredentials,
  type InstallSource,
} from "@/app/(dashboard)/fleets/new/install-flow";
import { INSTALL_STEP, type InstallStepId } from "@/lib/streaming/install-steps";

// A gallery entry is the only install source in M103 (template-only). These
// fixtures mirror GET /v1/workspaces/{ws}/fleet-templates: requirements are
// nested, and `visibility` keys the create body off the tier.
const PLATFORM_ENTRY: InstallSource = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  visibility: "platform",
  source_ref: "platform/github-pr-reviewer",
  requirements: {
    credentials: ["github"],
    tools: ["github_review_comment"],
    network_hosts: ["api.github.com"],
    trigger_present: true,
  },
  required_credentials_reasons: { github: "review pull requests" },
  support_files: [],
};

describe("requirementsOf — normalises the gallery entry", () => {
  it("a platform template reports its nested requirements + curated reasons", () => {
    const r = requirementsOf(PLATFORM_ENTRY);
    expect(r.name).toBe("GitHub PR reviewer");
    expect(r.credentials).toEqual(["github"]);
    expect(r.credentialReasons).toEqual({ github: "review pull requests" });
    expect(r.tools).toEqual(["github_review_comment"]);
    expect(r.networkHosts).toEqual(["api.github.com"]);
    expect(r.triggerPresent).toBe(true);
  });

  it("a template with no TRIGGER.md reports triggerPresent=false (skill-only fallback)", () => {
    const r = requirementsOf({
      ...PLATFORM_ENTRY,
      requirements: { ...PLATFORM_ENTRY.requirements, trigger_present: false },
    });
    expect(r.triggerPresent).toBe(false);
  });

  it("a tenant template's empty reasons map falls back to an empty map (generic copy)", () => {
    // Tenant rows carry no per-credential reason (the importer derives none), so
    // the gate must read an empty map rather than feed undefined into ConnectGate
    // (where reasons[credential] would otherwise throw).
    const r = requirementsOf({
      id: "01932d4e-7c10-7a3a-9f00-000000000001",
      name: "Internal ops",
      description: "Tenant template.",
      visibility: "tenant",
      source_ref: "tenant/01932d4e",
      requirements: { credentials: ["github"], tools: [], network_hosts: [], trigger_present: true },
      required_credentials_reasons: {},
      support_files: [],
    });
    expect(r.credentialReasons).toEqual({});
  });
});

describe("unmetCredentials + readyToCreate — the connect gate", () => {
  it("lists the required credentials not present in the vault", () => {
    expect(unmetCredentials(["github", "zoho"], ["github"])).toEqual(["zoho"]);
    expect(readyToCreate(["github"], ["github"])).toBe(true);
    expect(readyToCreate(["github"], [])).toBe(false);
  });

  it("an unreadable vault (null) gates nothing — the server's 424 stays authoritative", () => {
    expect(unmetCredentials(["github"], null)).toEqual([]);
    expect(readyToCreate(["github"], null)).toBe(true);
  });
});

describe("stepLine — SSE step → rendered line", () => {
  it("maps each step to its tone + glyph", () => {
    expect(stepLine(INSTALL_STEP.CREATING).tone).toBe("run");
    expect(stepLine(INSTALL_STEP.PROVISIONING).tone).toBe("run");
    expect(stepLine(INSTALL_STEP.READY).tone).toBe("ok");
    expect(stepLine(INSTALL_STEP.READY).glyph).toBe(STATE_GLYPH.ok);
    expect(stepLine(INSTALL_STEP.ERROR).tone).toBe("err");
  });

  it("an unknown step falls back to a running line labelled with the step id", () => {
    // Forward-compat: a step the UI does not yet model still renders, not crash.
    const line = stepLine("future_step" as InstallStepId);
    expect(line.tone).toBe("run");
    expect(line.text).toBe("future_step");
  });
});

describe("flowError — threads the action verb through the shared presenter", () => {
  it("produces a human string for a typed failure", () => {
    const msg = flowError({ errorCode: "UZ-FLEET-409", error: "name taken" }, "create the fleet");
    expect(typeof msg).toBe("string");
    expect(msg.length).toBeGreaterThan(0);
  });
});
