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

const SNAPSHOT = {
  bundle_id: "bnd_1",
  name: "acme/repo",
  source_kind: "github" as const,
  source_ref: "acme/repo",
  validation_status: "ok",
  content_hash: "h",
  snapshot_key: "k",
  support_files: [],
};

describe("requirementsOf — normalises each source", () => {
  it("a template reports catalog metadata, trigger present, no default name", () => {
    const r = requirementsOf({
      kind: "template",
      template: {
        id: "t",
        name: "T",
        description: "d",
        required_credentials: ["github"],
        required_credentials_reasons: { github: "review pull requests" },
        required_tools: ["x"],
        network_hosts: ["h"],
      },
    });
    expect(r.name).toBe("T");
    expect(r.credentials).toEqual(["github"]);
    expect(r.credentialReasons).toEqual({ github: "review pull requests" });
    expect(r.triggerPresent).toBe(true);
    expect(r.defaultName).toBeUndefined();
  });

  it("a template that omits required_credentials_reasons defaults to an empty map", () => {
    // A cached response, an old backend, or a mock can drop the field despite
    // the client cast; the `?? {}` guard keeps the gate on generic copy rather
    // than feeding undefined into ConnectGate (where reasons[credential] throws).
    const r = requirementsOf({
      kind: "template",
      template: {
        id: "t",
        name: "T",
        description: "d",
        required_credentials: ["github"],
        required_tools: [],
        network_hosts: [],
      },
    });
    expect(r.credentialReasons).toEqual({});
  });

  it("a github snapshot carries its parsed requirements, default name, and trigger flag", () => {
    const source: InstallSource = {
      kind: "github",
      snapshot: {
        ...SNAPSHOT,
        requirements: { credentials: ["a"], tools: ["b"], network_hosts: ["c"], support_files: [], trigger_present: false },
      },
    };
    const r = requirementsOf(source);
    expect(r.credentials).toEqual(["a"]);
    expect(r.defaultName).toBe("acme/repo");
    expect(r.triggerPresent).toBe(false);
  });

  it("a paste source has no declared requirements (parsed server-side at create)", () => {
    const r = requirementsOf({ kind: "paste", sourceMarkdown: "---\n---\n" });
    expect(r.credentials).toEqual([]);
    expect(r.tools).toEqual([]);
    expect(r.triggerPresent).toBe(true);
  });
});

describe("unmetCredentials + readyToCreate — the connect-to-continue gate", () => {
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
    const msg = flowError({ errorCode: "UZ-BUNDLE-004", error: "no SKILL.md" }, "import the template");
    expect(typeof msg).toBe("string");
    expect(msg.length).toBeGreaterThan(0);
  });
});
