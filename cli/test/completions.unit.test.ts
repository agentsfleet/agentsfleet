// Completion scripts advertise the same platform commands help does: a
// generated script is a snapshot of this credential's visibility.

import { describe, expect, test } from "bun:test";

import { rows, withKey } from "./helpers-discovery.ts";

const SHELLS = ["bash", "zsh", "fish", "powershell"] as const;
const DESCRIBED_SHELLS = ["zsh", "fish", "powershell"] as const;
const MODELS_FAMILY = "Manage the shared model catalogue";
const HIDDEN_FAMILIES = [
  "Manage vault-backed platform defaults",
  "Manage platform execution hosts",
  "Manage the platform fleet library",
] as const;
const PLATFORM_GROUP = "Administer platform resources with your existing operator scopes";
const WRITE_ONLY_LEAVES = ["rotate-token", "cordon", "self-test"] as const;

const script = async (scopes: ReadonlyArray<string>, shell: string): Promise<string> =>
  (await withKey(scopes, ["--completions", shell])).out;

describe("scoped_help_suggestions_and_four_shell_completions_agree", () => {
  test("model:read: help, suggestions and every script name only platform models", async () => {
    const scopes = ["model:read"];
    expect(rows((await withKey(scopes, ["--help"])).out)).toContain("platform");
    expect((await withKey(scopes, ["platfrom"])).err).toContain("platform");

    const bash = await script(scopes, "bash");
    expect(bash).toContain("_agentsfleet_platform()");
    expect(bash).toContain("_agentsfleet_platform_models()");
    for (const hidden of ["_agentsfleet_platform_keys()", "_agentsfleet_platform_runners()", "_agentsfleet_platform_fleet_library()"]) {
      expect(bash).not.toContain(hidden);
    }
    for (const shell of DESCRIBED_SHELLS) {
      const text = await script(scopes, shell);
      expect(text).toContain(MODELS_FAMILY);
      for (const hidden of HIDDEN_FAMILIES) expect(text).not.toContain(hidden);
    }
    for (const shell of SHELLS) {
      const text = await script(scopes, shell);
      for (const leaf of WRITE_ONLY_LEAVES) expect(text).not.toContain(leaf);
    }
  });

  test("no scopes: no script and no suggestion advertises platform", async () => {
    expect((await withKey([], ["platfrom"])).err).not.toContain("platform");
    expect(await script([], "bash")).not.toContain("_agentsfleet_platform");
    for (const shell of DESCRIBED_SHELLS) expect(await script([], shell)).not.toContain(PLATFORM_GROUP);
  });
});
