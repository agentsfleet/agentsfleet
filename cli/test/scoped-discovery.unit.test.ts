// Scoped help: what `platform` advertises follows the scopes the server
// reports for the credential in play, a read that fails or is ineligible
// advertises nothing, and hiding a command never stops it reaching the server.

import { describe, expect, test } from "bun:test";

import {
  ENV_KEY, EXIT_OK, EXIT_SERVER, EXIT_USAGE, STORED_TOKEN,
  RUNNING_AS_ROOT, identityReads, rows, run, withKey, type Setup,
} from "./helpers-discovery.ts";

// Each server closure, and exactly what it should advertise per family.
const MATRIX: ReadonlyArray<{ readonly scopes: ReadonlyArray<string>; readonly families: Readonly<Record<string, ReadonlyArray<string>>> }> = [
  { scopes: [], families: {} },
  { scopes: ["platform-key:read"], families: { keys: ["list"] } },
  { scopes: ["platform-key:admin", "platform-key:read"], families: { keys: ["list", "use", "delete"] } },
  { scopes: ["model:read"], families: { models: ["list", "show"] } },
  { scopes: ["model:admin", "model:read"], families: { models: ["list", "show", "create", "update", "delete"] } },
  { scopes: ["runner:read"], families: { runners: ["list", "show", "events", "leases"] } },
  { scopes: ["runner:enroll"], families: { runners: ["enroll"] } },
  {
    scopes: ["runner:write", "runner:read"],
    families: { runners: ["list", "show", "update", "rotate-token", "delete", "events", "leases", "cordon", "drain", "revoke", "self-test"] },
  },
  { scopes: ["platform-library:write"], families: { "fleet-library": ["list", "show", "stage", "update", "publish", "unpublish", "delete"] } },
  { scopes: ["workspace:any"], families: {} },
];

describe("platform_discovery_matches_effective_scope_matrix", () => {
  for (const row of MATRIX) {
    test(`[${row.scopes.join(", ")}] advertises exactly its families and leaves`, async () => {
      const families = Object.keys(row.families);
      const root = await withKey(row.scopes, ["--help"]);
      expect(root.code).toBe(EXIT_OK);
      expect(rows(root.out).includes("platform")).toBe(families.length > 0);

      const platform = await withKey(row.scopes, ["platform", "--help"]);
      expect([...rows(platform.out)].sort()).toEqual([...families].sort());

      for (const [family, leaves] of Object.entries(row.families)) {
        const help = await withKey(row.scopes, ["platform", family, "--help"]);
        expect([...rows(help.out)].sort()).toEqual([...leaves].sort());
      }
    });
  }
});

describe("discovery_refuses_malformed_expired_and_unavailable_identity", () => {
  const signedOut = () => run({ argv: ["--help"] });
  const signedOutTypo = () => run({ argv: ["platfrom"] });

  const abortable = (_url: string, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
    });

  const cases: ReadonlyArray<readonly [string, Partial<Setup>]> = [
    ["a body without scopes", { scopes: () => ({ user_id: "u" }) }],
    ["a list with a non-string member", { scopes: () => ({ scopes: ["model:read", 7] }) }],
    ["a 401", { fetchOverride: async () => new Response(JSON.stringify({ code: "UZ-AUTH-003" }), { status: 401, headers: { "content-type": "application/problem+json" } }) }],
    ["a read that never answers", { fetchOverride: abortable, discoveryTimeoutMs: 50 }],
    ["a network error", { fetchOverride: async () => { throw new TypeError("fetch failed"); } }],
  ];

  for (const [label, setup] of cases) {
    test(`${label} advertises nothing and changes no output`, async () => {
      const baseline = await signedOut();
      const started = performance.now();
      const help = await run({ argv: ["--help"], env: { AGENTSFLEET_API_KEY: ENV_KEY }, ...setup });
      // Under the 2000 ms default timeout, so a read that ignores the timeout
      // seam still fails; the room above 1000 ms is for coverage runs.
      expect(performance.now() - started).toBeLessThan(1_900);
      expect(help.code).toBe(EXIT_OK);
      expect(rows(help.out)).not.toContain("platform");
      expect(help.out).toBe(baseline.out);
      expect(help.err).toBe(baseline.err);

      const typo = await run({ argv: ["platfrom"], env: { AGENTSFLEET_API_KEY: ENV_KEY }, ...setup });
      expect(typo.code).toBe(EXIT_USAGE);
      expect(typo.err).toBe((await signedOutTypo()).err);
      expect(typo.err).not.toContain("platform");
    });
  }

  test("a signed-out run makes no identity read", async () => {
    const help = await signedOut();
    expect(identityReads(help.calls)).toEqual([]);
    expect(rows(help.out)).not.toContain("platform");
  });
});

describe("discovery_does_not_reuse_another_identity_or_target", () => {
  const A = "http://a.test";
  const B = "http://b.test";

  test("an environment key outranks the stored login", async () => {
    const help = await run({ argv: ["--help"], env: { AGENTSFLEET_API_KEY: ENV_KEY }, stored: { apiUrl: A }, scopes: () => ({ scopes: [] }) });
    expect(identityReads(help.calls).map((call) => call.bearer)).toEqual([ENV_KEY]);
  });

  test("two runs with different credentials build different trees", async () => {
    const scopes = (bearer: string | null) => ({ scopes: bearer === "agt_t_operator" ? ["model:read"] : [] });
    const operator = await run({ argv: ["--help"], env: { AGENTSFLEET_API_KEY: "agt_t_operator" }, scopes });
    const ordinary = await run({ argv: ["--help"], env: { AGENTSFLEET_API_KEY: "agt_t_ordinary" }, scopes });
    expect(rows(operator.out)).toContain("platform");
    expect(rows(ordinary.out)).not.toContain("platform");
  });

  test("a stored login is read only by the deployment that minted it", async () => {
    const own = await run({ argv: ["--api", A, "--help"], stored: { apiUrl: A } });
    const other = await run({ argv: ["--api", B, "--help"], stored: { apiUrl: A } });
    expect(identityReads(own.calls).map((call) => call.origin)).toEqual([A]);
    expect(identityReads(own.calls).map((call) => call.bearer)).toEqual([STORED_TOKEN]);
    expect(identityReads(other.calls)).toEqual([]);
  });

  test("an environment key never goes to a host named only by --api", async () => {
    const flagOnly = await run({ argv: ["--api", B, "--help"], env: { AGENTSFLEET_API_KEY: ENV_KEY } });
    const noFallback = await run({ argv: ["--api", B, "--help"], env: { AGENTSFLEET_API_KEY: ENV_KEY }, stored: { apiUrl: B } });
    const configured = await run({ argv: ["--help"], env: { AGENTSFLEET_API_KEY: ENV_KEY, AGENTSFLEET_API_URL: B } });
    expect(identityReads(flagOnly.calls)).toEqual([]);
    expect(identityReads(noFallback.calls)).toEqual([]);
    expect(identityReads(configured.calls).map((call) => [call.origin, call.bearer])).toEqual([[B, ENV_KEY]]);
  });

  test.each([[["--help"]], [["platform", "--help"]], [["--completions", "zsh"]], [["platfrom"]], [["--api", A, "--help"]]])(
    "an unbound stored login sends no read for %p",
    async (argv) => {
      const outcome = await run({ argv, stored: { apiUrl: null } });
      expect(identityReads(outcome.calls)).toEqual([]);
    },
  );

  test.each([[["fleet", "list"]], [["fleet", "--help"]], [["platform", "keys", "list"]]])(
    "%p makes no identity read",
    async (argv) => {
      const outcome = await withKey(["platform-key:read"], argv);
      expect(identityReads(outcome.calls)).toEqual([]);
    },
  );

  test.skipIf(RUNNING_AS_ROOT)("a stored login that cannot be read is no credential at all", async () => {
    const outcome = await run({ argv: ["--help"], storedUnreadable: true });
    expect(identityReads(outcome.calls)).toEqual([]);
    expect(rows(outcome.out)).not.toContain("platform");
  });

  test.each([[["platform", "keys", "list", "--help"]], [["platform", "keys"]], [["help", "platform"]]])(
    "%p is a discovery request",
    async (argv) => {
      const outcome = await withKey(["platform-key:read"], argv);
      expect(identityReads(outcome.calls)).toHaveLength(1);
    },
  );

  test("one discovery request makes exactly one identity read", async () => {
    const outcome = await withKey(["model:read"], ["--help"]);
    expect(identityReads(outcome.calls)).toHaveLength(1);
  });
});

describe("hidden_command_and_stale_completion_cannot_bypass_authorization", () => {
  test("an unadvertised command still parses and the server still refuses it", async () => {
    const outcome = await withKey([], ["platform", "keys", "list"]);
    expect(outcome.calls.map((call) => `${call.method} ${call.path}`)).toEqual(["GET /v1/admin/platform-keys"]);
    expect(outcome.err).toMatch(/^ {2}code {10}UZ-AUTH-022$/m);
    expect(outcome.code).toBe(EXIT_SERVER);
  });
});

describe("discovery_writes_no_credential_or_identity_to_diagnostics", () => {
  test("neither the key nor the identity body reaches either stream", async () => {
    const outcome = await run({
      argv: ["--help", "--log-level", "debug"],
      env: { AGENTSFLEET_API_KEY: "agt_t_SENTINEL" },
      scopes: () => ({ user_id: "u", email: "probe@example.test", scopes: ["model:read"] }),
    });
    expect(identityReads(outcome.calls)).toHaveLength(1);
    // Positive control: debug output really was on, and it reached stderr.
    expect(outcome.err).toContain("endpoint=/v1/users/me");
    for (const stream of [outcome.out, outcome.err]) {
      expect(stream).not.toContain("agt_t_SENTINEL");
      expect(stream).not.toContain("probe@example.test");
    }
  });
});
