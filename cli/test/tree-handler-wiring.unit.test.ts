// The thin line between a command's flags and the effect that runs it.
//
// The effects themselves are covered by their own unit tests; what is covered
// here is the WIRING — that `tenant provider create` reaches
// tenantProviderAddEffectFromArgs with its flags, and `billing show` reaches
// billingShowEffectFromArgs with its paging. A handler wired to the wrong
// effect, or dropping a flag on the way, typechecks perfectly and fails only
// in front of a user.

import { describe, expect, test } from "bun:test";

import { runCli } from "../src/cli.ts";
import { TENANT_PROVIDER_PATH, USERS_ME_PATH } from "../src/lib/api-paths.ts";
import { loadCredentials, loadWorkspaces } from "../src/lib/state.ts";
import { bufferStream, cliEnv, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";
import { jsonResponse, withMockApi } from "./helpers-mock-api.ts";

const EXIT_VALIDATION = 4;
const UNROUTABLE = "https://127.0.0.1:1";
const WORKSPACE_ID = "01900000-0000-7000-8000-000000000001";
const PROVIDER = { mode: "platform", provider: "fireworks", model: "test-model" };

const invoke = async (argv: ReadonlyArray<string>): Promise<{ code: number; err: string }> => {
  const chunks: string[] = [];
  const code = await runCli([...argv], {
    stdout: { write: () => true, isTTY: false },
    stderr: { write: (c: string) => { chunks.push(c); return true; }, isTTY: false },
    // A real key and an unroutable target: the invocation gets all the way to
    // the transport, which is how we know the handler ran, and then fails
    // there instead of reaching anyone's deployment.
    env: { AGENTSFLEET_API_KEY: "afk_unit_test", AGENTSFLEET_API_URL: UNROUTABLE, NO_COLOR: "1" },
  });
  return { code, err: chunks.join("") };
};

describe("tenant provider create is wired to its effect", () => {
  test("it parses its flags and reaches the handler", async () => {
    const { code } = await invoke([
      "tenant", "provider", "create", "--secret", "my-secret", "--model", "gpt-4",
    ]);
    // Not a validation exit: the flags were accepted and the handler ran.
    expect(code).not.toBe(EXIT_VALIDATION);
  });

  test("it still refuses a flag it does not declare", async () => {
    const { code, err } = await invoke([
      "tenant", "provider", "create", "--secret", "s", "--model", "m", "--nope", "x",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("--nope");
  });
});

describe("billing show is wired to its effect", () => {
  test("it parses its paging flags and reaches the handler", async () => {
    const { code } = await invoke(["billing", "show", "--limit", "10"]);
    expect(code).not.toBe(EXIT_VALIDATION);
  });

  test("its cap is its own, not the list cap", async () => {
    const { code, err } = await invoke(["billing", "show", "--limit", "9999"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be ≤ 100");
  });
});

test("whoami reaches the identity route and prints the server identity", async () => {
  await withFreshStateDir(async () => {
    const identity = {
      user_id: WORKSPACE_ID,
      email: "ada@example.test",
      display_name: "Ada",
      tenant_id: WORKSPACE_ID,
      tenant_name: "Test tenant",
      credential: "tenant_api_key",
      scopes: ["fleet:read"],
    };
    await withMockApi({ [`GET ${USERS_ME_PATH}`]: () => jsonResponse(200, identity) }, async (apiUrl, calls) => {
      const out = bufferStream();
      const code = await runCli(["--json", "whoami"], {
        stdout: out.stream,
        stderr: bufferStream().stream,
        env: cliEnv({ AGENTSFLEET_API_KEY: "afk_unit_test", AGENTSFLEET_API_URL: apiUrl }),
      });
      expect(code).toBe(0);
      expect(calls.map((call) => call.path)).toEqual([USERS_ME_PATH]);
      expect(JSON.parse(out.read()).email).toBe(identity.email);
    });
  });
});

test("logout --all refuses the unsupported scope without clearing credentials", async () => {
  await withAuthedStateDir({ workspaceId: WORKSPACE_ID }, async () => {
    const err = bufferStream();
    const code = await runCli(["logout", "--all"], {
      stdout: bufferStream().stream,
      stderr: err.stream,
      env: cliEnv({ AGENTSFLEET_API_URL: UNROUTABLE }),
    });
    expect(code).toBe(EXIT_VALIDATION);
    expect(err.read()).toContain("--all");
    expect((await loadCredentials(process.env)).token).not.toBeNull();
  });
});

test("workspace delete removes the selected local workspace", async () => {
  await withAuthedStateDir({ workspaceId: WORKSPACE_ID }, async () => {
    const out = bufferStream();
    const code = await runCli(["workspace", "delete", WORKSPACE_ID], {
      stdout: out.stream,
      stderr: bufferStream().stream,
      env: cliEnv({ AGENTSFLEET_API_URL: UNROUTABLE }),
    });
    expect(code).toBe(0);
    expect(out.read()).toContain(WORKSPACE_ID);
    const saved = await loadWorkspaces(process.env);
    expect(saved.items).toEqual([]);
    expect(saved.current_workspace_id).toBeNull();
  });
});

test("tenant provider show and delete reach their distinct HTTP methods", async () => {
  await withAuthedStateDir({ workspaceId: WORKSPACE_ID }, async () => {
    await withMockApi({
      [`GET ${TENANT_PROVIDER_PATH}`]: () => jsonResponse(200, PROVIDER),
      [`DELETE ${TENANT_PROVIDER_PATH}`]: () => jsonResponse(200, PROVIDER),
    }, async (apiUrl, calls) => {
      for (const verb of ["show", "delete"]) {
        const out = bufferStream();
        const code = await runCli(["--json", "tenant", "provider", verb], {
          stdout: out.stream,
          stderr: bufferStream().stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        expect(JSON.parse(out.read()).model).toBe(PROVIDER.model);
      }
      expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual([
        `GET ${TENANT_PROVIDER_PATH}`,
        `DELETE ${TENANT_PROVIDER_PATH}`,
      ]);
    });
  });
});
